//! SigLIP-2 image + text encoder pair (Google, ICCV 2025).
//!
//! Sigmoid-loss CLIP-family model. SigLIP-2 outperforms OpenAI
//! CLIP-ViT-B/32 on text→image retrieval at every model scale and
//! is the recommended modern replacement.
//!
//! ## Two encoders, one space
//!
//! Image and text encoders produce embeddings in the SAME 768-dim
//! space — they're trained together. Mixing them across encoder
//! families (e.g. SigLIP image + CLIP text) doesn't work; you must
//! use both halves together for text→image search to be meaningful.
//!
//! This is why both encoders ship together in this file. The
//! encoder picker in Settings either selects "SigLIP-2" (both halves)
//! or "CLIP" (both halves) for the text→image search.
//!
//! ## Tokenizer (verified 2026-04-26)
//!
//! Gemma 2 SentencePiece — 256k vocab, 34 MB tokenizer.json.
//! Loaded via the `tokenizers` crate. Special tokens:
//!   - `<pad>` = 0
//!   - `<eos>` = 1 (auto-appended by tokenizer)
//!   - `<bos>` = 2 (NOT prepended — `add_bos_token=false`)
//!   - `<unk>` = 3
//! Pad to exactly max_seq_length=64 with id 0. The model's position
//! embedding is fixed-size 64; longer sequences crash with an
//! out-of-range Gather at runtime.
//!
//! ## Preprocessing — image side (verified 2026-04-26)
//!
//! - Convert to RGB
//! - Resize to **exact 256×256** (bilinear, NO aspect preservation —
//!   SigLIP-2 was trained on stretched-square inputs). NO center-crop.
//! - Rescale by 1/255 → [0, 1]
//! - Normalize with mean=std=[0.5, 0.5, 0.5] → maps to [-1, 1]
//! - CHW layout → [1, 3, 256, 256] float32
//!
//! Distinct from CLIP's bicubic-shortest-edge-224-then-crop and
//! DINOv2's bicubic-shortest-edge-256-then-crop. Each encoder must
//! be fed the distribution + geometry it was trained on.
//!
//! ## ONNX I/O (verified 2026-04-26)
//!
//! - **vision_model.onnx**: input `pixel_values` [1,3,256,256] f32;
//!   output `pooler_output` [1,768] f32. The pooler is a MAP
//!   (multi-head attention pooling) head — do NOT try to extract
//!   CLS token from `last_hidden_state` (it doesn't exist for
//!   SigLIP).
//! - **text_model.onnx**: input `input_ids` [1,64] int64 ONLY (no
//!   attention_mask — fixed-length path); output `pooler_output`
//!   [1,768] f32.
//!
//! ## Model URLs (verified 2026-04-26)
//!
//! `onnx-community/siglip2-base-patch16-256-ONNX`. Note the `-ONNX`
//! suffix — the non-suffixed variant returned 401.

use image::ImageReader;
use ort::{session::Session, value::Tensor};
use std::{error::Error, path::Path};
use tokenizers::Tokenizer;
use tracing::{debug, info, warn};

use super::encoder_text::pooling::normalize;
use super::encoders::{ImageEncoder, TextEncoder as TextEncoderTrait};

/// Vision tower ONNX. SigLIP-2-Base, 256×256 stretched-square input,
/// patch16. Verified 200 OK 2026-04-26.
pub const SIGLIP2_IMAGE_MODEL_URL: &str =
    "https://huggingface.co/onnx-community/siglip2-base-patch16-256-ONNX/resolve/main/onnx/vision_model.onnx";
pub const SIGLIP2_IMAGE_MODEL_FILENAME: &str = "siglip2_vision.onnx";

/// Text tower ONNX. Same checkpoint as image tower; produces text
/// embeddings in the shared 768-dim space. ~1.13 GB because the
/// Gemma 256k vocab makes the embedding matrix very large.
pub const SIGLIP2_TEXT_MODEL_URL: &str =
    "https://huggingface.co/onnx-community/siglip2-base-patch16-256-ONNX/resolve/main/onnx/text_model.onnx";
pub const SIGLIP2_TEXT_MODEL_FILENAME: &str = "siglip2_text.onnx";

/// Gemma 2 SentencePiece tokenizer in HF tokenizer.json format
/// (~34 MB).
pub const SIGLIP2_TOKENIZER_URL: &str =
    "https://huggingface.co/onnx-community/siglip2-base-patch16-256-ONNX/resolve/main/tokenizer.json";
pub const SIGLIP2_TOKENIZER_FILENAME: &str = "siglip2_tokenizer.json";

pub const SIGLIP2_ENCODER_ID: &str = "siglip2_base";

const IMG_SIZE: u32 = 256;
const HIDDEN: usize = 768;
const MAX_SEQ: usize = 64;
const PAD_TOKEN_ID: i64 = 0;

// =====================================================================
// Image encoder
// =====================================================================

pub struct Siglip2ImageEncoder {
    session: Session,
}

impl Siglip2ImageEncoder {
    pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
        Self::new_with_intra(model_path, super::ort_session::DEFAULT_INTRA_THREADS)
    }

    /// Phase 12c — explicit intra-thread count. See ort_session.rs.
    pub fn new_with_intra(model_path: &Path, intra_threads: usize) -> Result<Self, Box<dyn Error>> {
        info!("=== Initialising SigLIP-2 image encoder (intra={intra_threads}) ===");
        info!("model: {}", model_path.display());
        let session = super::ort_session::build_tuned_session_with_intra(
            "siglip2_image",
            model_path,
            intra_threads,
        )?;
        Ok(Self { session })
    }

    fn preprocess(&self, image_path: &Path) -> Result<ndarray::Array4<f32>, Box<dyn Error>> {
        // Stretched-square resize to exactly 256×256, bilinear. SigLIP-2
        // was trained without aspect preservation; do not center-crop
        // — there's no crop in the canonical pipeline.
        let img = ImageReader::open(image_path)?
            .with_guessed_format()?
            .decode()?
            .to_rgb8();
        // Phase 12e — fast_image_resize Lanczos3 instead of image-rs's
        // Triangle (bilinear). Slight quality upgrade (Lanczos3 ≥
        // bilinear) plus the ~7-13× speedup. SigLIP-2's stretched-square
        // resize doesn't preserve aspect, so the filter quality matters
        // less than for aspect-preserving resizes — but free is free.
        let resized = super::preprocess::fast_resize_rgb8(&img, IMG_SIZE, IMG_SIZE, "siglip2_image");

        let plane = (IMG_SIZE * IMG_SIZE) as usize;
        let mut tensor: Vec<f32> = Vec::with_capacity(3 * plane);
        let mut r = Vec::with_capacity(plane);
        let mut g = Vec::with_capacity(plane);
        let mut b = Vec::with_capacity(plane);
        for px in resized.pixels() {
            r.push(px[0] as f32 / 255.0);
            g.push(px[1] as f32 / 255.0);
            b.push(px[2] as f32 / 255.0);
        }
        tensor.extend(r);
        tensor.extend(g);
        tensor.extend(b);

        // SigLIP normalisation — mean=std=0.5 maps [0, 1] → [-1, 1].
        let mean = [0.5_f32; 3];
        let std = [0.5_f32; 3];
        for c in 0..3 {
            for i in 0..plane {
                let idx = c * plane + i;
                tensor[idx] = (tensor[idx] - mean[c]) / std[c];
            }
        }

        Ok(ndarray::Array4::from_shape_vec(
            (1, 3, IMG_SIZE as usize, IMG_SIZE as usize),
            tensor,
        )?)
    }
}

impl ImageEncoder for Siglip2ImageEncoder {
    #[tracing::instrument(name = "siglip2.encode_image", skip(self), fields(path = %image_path.display()))]
    fn encode(&mut self, image_path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_array = self.preprocess(image_path)?;
        let shape = [1usize, 3, IMG_SIZE as usize, IMG_SIZE as usize];
        let (data, _offset) = input_array.into_raw_vec_and_offset();
        let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;

        let outputs = self.session.run(ort::inputs![
            "pixel_values" => onnx_input
        ])?;

        // Use `pooler_output` — the MAP (multi-head attention pooling)
        // head's projected output. NOT `last_hidden_state[:, 0, :]`;
        // SigLIP doesn't use CLS-token pooling.
        let extract = |name: &str| -> Option<Vec<f32>> {
            outputs.get(name).and_then(|t| {
                t.try_extract_tensor::<f32>()
                    .ok()
                    .map(|(_, view)| view.to_vec())
            })
        };
        let raw = extract("pooler_output")
            .or_else(|| extract("image_embeds"))
            .ok_or("SigLIP-2 image: no recognised output name (expected pooler_output)")?;

        debug!("SigLIP-2 image embedding length: {}", raw.len());
        Ok(normalize(&raw))
    }

    fn embedding_dim(&self) -> usize {
        HIDDEN
    }

    fn id(&self) -> &'static str {
        SIGLIP2_ENCODER_ID
    }
}

// =====================================================================
// Text encoder
// =====================================================================

pub struct Siglip2TextEncoder {
    session: Session,
    tokenizer: Tokenizer,
    /// SigLIP-2's max sequence length is 64 tokens (per the model's
    /// position embedding table — exceeding crashes with Gather
    /// out-of-range at runtime).
    max_seq_length: usize,
}

impl Siglip2TextEncoder {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initialising SigLIP-2 text encoder ===");
        info!("model: {}", model_path.display());
        info!("tokenizer: {}", tokenizer_path.display());

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("SigLIP-2 tokenizer load failed: {e}"))?;
        // R4 — shared M2-tuned session builder.
        let session = super::ort_session::build_tuned_session(
            "siglip2_text",
            model_path,
        )?;

        let mut encoder = Self {
            session,
            tokenizer,
            max_seq_length: MAX_SEQ,
        };

        // Phase 12d — real-input pre-warm. Same pattern as the CLIP
        // text encoder. Without this, the first SigLIP-2 text query
        // pays ORT's first-inference initialisation tax (perf-1777226449
        // captured this as a 2.41s outlier on `ipc.get_fused_semantic_search`).
        // Running encode("warmup") here moves the cost off the user's
        // interactive path. We don't care about the result.
        use crate::similarity_and_semantic_search::encoders::TextEncoder as TextEncoderTrait;
        match encoder.encode("warmup") {
            Ok(_) => info!("SigLIP-2 text encoder real-input pre-warm complete"),
            Err(e) => warn!("SigLIP-2 text encoder pre-warm inference failed: {e}"),
        }

        Ok(encoder)
    }

    /// Tokenise + pad/truncate to `max_seq_length`. The Gemma
    /// tokenizer auto-appends `<eos>=1` (configured via
    /// `add_eos_token=true` in tokenizer_config.json) — we do NOT
    /// prepend `<bos>` ourselves.
    fn encode_text(&self, text: &str) -> Result<Vec<i64>, Box<dyn Error>> {
        let encoded = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("SigLIP-2 tokenize failed: {e}"))?;
        let mut ids: Vec<i64> = encoded.get_ids().iter().map(|&i| i as i64).collect();

        // Hard-cap at 64 (the position embedding's fixed size). Pad
        // with `<pad>=0` to exactly 64 — the ONNX graph's position
        // table has no growth room.
        if ids.len() > self.max_seq_length {
            ids.truncate(self.max_seq_length);
        } else {
            ids.resize(self.max_seq_length, PAD_TOKEN_ID);
        }
        Ok(ids)
    }

    /// Borrow the tokenizer + max_seq_length for diagnostics.
    pub fn tokenizer_for_diagnostic(&self) -> &Tokenizer {
        &self.tokenizer
    }
    pub fn max_seq_length(&self) -> usize {
        self.max_seq_length
    }
}

impl TextEncoderTrait for Siglip2TextEncoder {
    #[tracing::instrument(name = "siglip2.encode_text", skip(self, text), fields(query_len = text.len()))]
    fn encode(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_ids = self.encode_text(text)?;
        let input_ids_tensor: Tensor<i64> =
            Tensor::from_array(([1usize, self.max_seq_length], input_ids))?;

        // SigLIP-2 text encoder takes ONLY input_ids — the ONNX
        // export inlined the position lookup as a fixed-length Slice,
        // so attention_mask is unused. Pass anything else and the
        // session.run errors out.
        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor
        ])?;

        let extract = |name: &str| -> Option<Vec<f32>> {
            outputs.get(name).and_then(|t| {
                t.try_extract_tensor::<f32>()
                    .ok()
                    .map(|(_, view)| view.to_vec())
            })
        };
        // Use pooler_output — same as the image branch. Both branches
        // project into the shared 768-d joint space via this head.
        let raw = extract("pooler_output")
            .or_else(|| extract("text_embeds"))
            .ok_or("SigLIP-2 text: no recognised output name (expected pooler_output)")?;

        debug!("SigLIP-2 text embedding length: {}", raw.len());
        Ok(normalize(&raw))
    }

    fn embedding_dim(&self) -> usize {
        HIDDEN
    }

    fn id(&self) -> &'static str {
        SIGLIP2_ENCODER_ID
    }
}
