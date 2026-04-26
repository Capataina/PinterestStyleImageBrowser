use ort::{session::Session, value::Tensor};
use std::{error::Error, path::Path};
use tokenizers::Tokenizer;
use tracing::{debug, info, warn};

use super::pooling::normalize;
// Trait import aliased to avoid the name collision with our concrete
// struct ClipTextEncoder. The trait lives in the sibling encoders
// module and is what the runtime dispatches against.
use crate::similarity_and_semantic_search::encoders::TextEncoder as TextEncoderTrait;

// CoreML is intentionally NOT used for the text encoder on macOS.
// The OpenAI CLIP text model is a transformer (12-layer, 8-head); CoreML's
// transformer node coverage is poor and its session-create overhead (6-15s)
// dominates the actual inference cost. Plain CPU is faster end-to-end.
//
// Image encoder (encoder.rs) keeps its accelerator hook because CLIP's
// CNN-heavy graph is what CoreML is good at — there it's a 5-10× win.

#[cfg(not(target_os = "macos"))]
use ort::execution_providers::CUDAExecutionProvider;

/// Text encoder for OpenAI CLIP ViT-B/32 (English-only).
///
/// Uses the SEPARATE `text_model.onnx` from
/// `Xenova/clip-vit-base-patch32` — NOT the multilingual distillation
/// (`sentence-transformers/clip-ViT-B-32-multilingual-v1`) we shipped
/// previously. The multilingual model is in a different embedding
/// space than the image encoder; using it for text→image search
/// produced effectively-random rankings, which is the bug class
/// "blue fish → Tristana" was an example of.
///
/// Tokenization: byte-level BPE via the HuggingFace `tokenizers`
/// crate. The `tokenizer.json` carries the full normalization +
/// special-tokens contract; `tokenizers` handles it all uniformly.
///
/// Inputs:
///   - `input_ids` [1, 77] int64 — ONLY input. The Xenova
///     `text_model.onnx` export bakes the causal/padding mask into
///     the graph from the input_ids themselves (it knows the pad
///     token is 49407 = EOS, and treats positions after the first
///     EOS as padding). Passing `attention_mask` errors with
///     "Invalid input name: attention_mask" at session.run time.
///     We still pad with id 49407 to fixed length 77; the model
///     handles the mask internally.
/// Output: `text_embeds` [1, 512] f32 (post-projection, post-LN)
/// L2-normalised before returning.
pub struct ClipTextEncoder {
    session: Session,
    tokenizer: Tokenizer,
    max_seq_length: usize,
    pad_token_id: i64,
}

/// OpenAI CLIP padding token. Verified from `Xenova/clip-vit-base-patch32`
/// `tokenizer_config.json`: `pad_token = <|endoftext|>` with id 49407.
const CLIP_PAD_TOKEN_ID: i64 = 49407;
const CLIP_MAX_SEQ_LENGTH: usize = 77;

impl ClipTextEncoder {
    /// Create a new ClipTextEncoder from ONNX model and tokenizer files.
    ///
    /// # Arguments
    /// * `model_path` - Path to the ONNX text model (e.g., `clip_text.onnx`)
    /// * `tokenizer_path` - Path to the tokenizer.json file (BPE)
    #[cfg(target_os = "macos")]
    fn build_session_with_accel(model_path: &Path) -> Result<Session, Box<dyn Error>> {
        // macOS: text encoder runs CPU-only because CoreML coverage is
        // poor for transformers (see top-of-file comment). Plain CPU
        // session creates in ~1-2s vs CoreML's 6-15s for the same model.
        info!("text encoder using CPU (CoreML skipped — poor transformer coverage)");
        let session = Session::builder()?.commit_from_file(model_path)?;
        Ok(session)
    }

    #[cfg(not(target_os = "macos"))]
    fn build_session_with_accel(model_path: &Path) -> Result<Session, Box<dyn Error>> {
        info!("trying CUDA execution provider for text encoder");
        let session = Session::builder()?
            .with_execution_providers([CUDAExecutionProvider::default().build()])?
            .commit_from_file(model_path)?;
        Ok(session)
    }

    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initializing CLIP text encoder (OpenAI English) ===");
        debug!("Model path: {:?}", model_path);
        debug!("Tokenizer path: {:?}", tokenizer_path);

        // Load tokenizer via the canonical HF crate. Handles BPE merges,
        // NFC normalization, lowercase, special-token wrapping, etc.
        info!("Loading BPE tokenizer...");
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("CLIP tokenizer load failed: {e}"))?;
        info!("Tokenizer loaded");

        // Build the ONNX session with the platform-appropriate
        // execution provider, falling back to CPU on any error.
        let session = match Self::build_session_with_accel(model_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("text encoder accelerator init failed ({e}); falling back to CPU");
                Session::builder()?.commit_from_file(model_path)?
            }
        };

        Ok(ClipTextEncoder {
            session,
            tokenizer,
            max_seq_length: CLIP_MAX_SEQ_LENGTH,
            pad_token_id: CLIP_PAD_TOKEN_ID,
        })
    }

    /// Borrow the tokenizer for the `tokenizer_output` diagnostic.
    /// Returns the raw HF Tokenizer — callers can use `.encode(text, true)`
    /// to get IDs + tokens without running ONNX inference.
    pub fn tokenizer_for_diagnostic(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Configured max_seq_length (77 for OpenAI CLIP).
    pub fn max_seq_length(&self) -> usize {
        self.max_seq_length
    }

    /// Inspect the model's input and output names (useful for debugging)
    pub fn inspect_model(&self) {
        debug!("Text Model inputs:");
        for input in self.session.inputs.iter() {
            debug!("  Name: {:?}", input.name);
        }

        debug!("\nText Model outputs:");
        for output in self.session.outputs.iter() {
            debug!("  Name: {:?}", output.name);
        }
    }

    /// Tokenize + pad/truncate to `max_seq_length`. Returns the
    /// fixed-length `input_ids` only — the Xenova `text_model.onnx`
    /// export does not accept an attention_mask input (the mask is
    /// derived inside the graph from the pad-token positions).
    fn tokenize_and_pad(&self, text: &str) -> Result<Vec<i64>, Box<dyn Error>> {
        let encoded = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("CLIP tokenize failed: {e}"))?;
        let mut ids: Vec<i64> = encoded.get_ids().iter().map(|&i| i as i64).collect();

        // Truncate or pad to exactly max_seq_length. Pad token = 49407
        // (= EOS = endoftext in OpenAI CLIP). The model recognises
        // these padding positions internally.
        if ids.len() > self.max_seq_length {
            ids.truncate(self.max_seq_length);
        } else {
            let pad_count = self.max_seq_length - ids.len();
            ids.extend(std::iter::repeat(self.pad_token_id).take(pad_count));
        }
        Ok(ids)
    }

    /// Encode a text string into a 512-dimensional embedding vector.
    ///
    /// # Returns
    /// A 512-dimensional L2-normalised embedding vector.
    #[tracing::instrument(name = "clip.encode_text", skip(self, text), fields(query_len = text.len()))]
    pub fn encode(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_ids = self.tokenize_and_pad(text)?;

        // Create the input tensor with shape [1, max_seq_length].
        let input_ids_tensor: Tensor<i64> =
            Tensor::from_array(([1usize, self.max_seq_length], input_ids))?;

        // OpenAI CLIP text_model.onnx (Xenova export):
        //   input:  input_ids ONLY — see the type-level comment above
        //           for why attention_mask is not accepted here.
        //   outputs: text_embeds [1, 512] (post-projection), last_hidden_state
        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor
        ])?;

        // Try `text_embeds` first (joint-space, post-projection). Fall
        // back to other names if a future export renames the output.
        let extract = |name: &str| -> Option<Vec<f32>> {
            outputs.get(name).and_then(|t| {
                t.try_extract_tensor::<f32>()
                    .ok()
                    .map(|(_, view)| view.to_vec())
            })
        };
        let raw = extract("text_embeds")
            .or_else(|| extract("pooler_output"))
            .or_else(|| extract("sentence_embedding"))
            .ok_or("CLIP text: no recognised output name (expected text_embeds)")?;

        // L2-normalise so cosine is well-conditioned. The Xenova
        // text_model.onnx outputs the post-projection embedding
        // without normalization.
        Ok(normalize(&raw))
    }

    /// Encode multiple texts in a batch (more efficient for multiple queries)
    pub fn encode_batch(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let batch_size = texts.len();

        let mut all_input_ids: Vec<i64> = Vec::with_capacity(batch_size * self.max_seq_length);

        for text in texts {
            let ids = self.tokenize_and_pad(text)?;
            all_input_ids.extend(ids);
        }

        let input_ids_tensor: Tensor<i64> =
            Tensor::from_array(([batch_size, self.max_seq_length], all_input_ids))?;

        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor
        ])?;

        let extract = |name: &str| -> Option<(Vec<usize>, Vec<f32>)> {
            outputs.get(name).and_then(|t| {
                t.try_extract_tensor::<f32>().ok().map(|(shape, view)| {
                    let s: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
                    (s, view.to_vec())
                })
            })
        };
        let (shape, data) = extract("text_embeds")
            .or_else(|| extract("pooler_output"))
            .ok_or("CLIP text batch: no recognised output name")?;

        // Expected shape: [batch_size, 512]
        if shape.len() == 2 && shape[1] == 512 {
            let mut embeddings = Vec::with_capacity(batch_size);
            for i in 0..batch_size {
                let start = i * 512;
                let end = start + 512;
                embeddings.push(normalize(&data[start..end].to_vec()));
            }
            return Ok(embeddings);
        }

        Err(format!(
            "CLIP text batch: unexpected output shape {:?}, expected [batch, 512]",
            shape
        )
        .into())
    }
}

/// Implement the new trait via delegation to the inherent methods.
/// This is the seam that lets the runtime hold a `Box<dyn TextEncoder>`
/// containing either ClipTextEncoder, the SigLIP2-Text encoder, or
/// any future text encoder.
impl TextEncoderTrait for ClipTextEncoder {
    fn encode(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
        ClipTextEncoder::encode(self, text)
    }
    fn embedding_dim(&self) -> usize {
        // OpenAI CLIP-ViT-B/32 outputs 512-d (text_config.hidden_size
        // and projection_dim are both 512).
        512
    }
    fn id(&self) -> &'static str {
        // Used as the database column suffix and the user-facing
        // label. Stable forever — changing this would orphan
        // existing embedding rows.
        "clip_vit_b_32"
    }
}
