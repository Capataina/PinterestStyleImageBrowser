//! SigLIP-2 image + text encoder pair (Google, ICCV 2023 + 2025).
//!
//! Sigmoid-loss CLIP-family model. Per the project-enhancement
//! agent's research (multiple ICLR/ICCV/CVPR papers), SigLIP-2
//! outperforms OpenAI CLIP-ViT-B/32 on text→image retrieval at
//! every model scale and is the recommended modern replacement.
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
//! ## Tokenizer
//!
//! SigLIP uses a SentencePiece tokenizer (different from CLIP's
//! WordPiece). We load it via the `tokenizers` crate from
//! HuggingFace, which is the canonical Rust-native loader for the
//! HF `tokenizer.json` format.
//!
//! ## Preprocessing (image side)
//!
//! - Resize to 224×224 (Lanczos3 — closest approximation to the
//!   reference's bicubic that the `image` crate offers).
//! - Normalise to `[-1, 1]` via mean=0.5, std=0.5 per channel
//!   (SigLIP's training stats — distinct from CLIP's ImageNet-derived
//!   stats and DINOv2's ImageNet stats; each encoder must be fed
//!   the distribution it was trained on).
//! - CHW layout for ONNX.
//!
//! ## Model URLs
//!
//! ONNX exports from the `onnx-community` HuggingFace org. If 404s,
//! check <https://huggingface.co/onnx-community> for the canonical
//! current export.

use image::ImageReader;
use ort::{session::Session, value::Tensor};
use std::{error::Error, path::Path};
use tokenizers::Tokenizer;
use tracing::{debug, info};

use super::encoder_text::pooling::normalize;
use super::encoders::{ImageEncoder, TextEncoder as TextEncoderTrait};

/// Vision tower ONNX. SigLIP-2-Base, 224×224, patch16.
/// `Xenova/siglip2-base-patch16-224` is the canonical pre-built ONNX
/// export; the previous `onnx-community/...` path returned 401.
pub const SIGLIP2_IMAGE_MODEL_URL: &str =
    "https://huggingface.co/Xenova/siglip2-base-patch16-224/resolve/main/onnx/vision_model.onnx";
pub const SIGLIP2_IMAGE_MODEL_FILENAME: &str = "model_siglip2_image.onnx";

/// Text tower ONNX. Same checkpoint as image tower; produces text
/// embeddings in the shared 768-dim space.
pub const SIGLIP2_TEXT_MODEL_URL: &str =
    "https://huggingface.co/Xenova/siglip2-base-patch16-224/resolve/main/onnx/text_model.onnx";
pub const SIGLIP2_TEXT_MODEL_FILENAME: &str = "model_siglip2_text.onnx";

/// SentencePiece-based tokenizer in HF tokenizer.json format.
pub const SIGLIP2_TOKENIZER_URL: &str =
    "https://huggingface.co/Xenova/siglip2-base-patch16-224/resolve/main/tokenizer.json";
pub const SIGLIP2_TOKENIZER_FILENAME: &str = "tokenizer_siglip2.json";

// =====================================================================
// Image encoder
// =====================================================================

pub struct Siglip2ImageEncoder {
    session: Session,
}

impl Siglip2ImageEncoder {
    pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initialising SigLIP-2 image encoder ===");
        info!("model: {}", model_path.display());
        let session = Session::builder()?.commit_from_file(model_path)?;
        Ok(Self { session })
    }

    fn preprocess(&self, image_path: &Path) -> Result<ndarray::Array4<f32>, Box<dyn Error>> {
        let img = ImageReader::open(image_path)?
            .with_guessed_format()?
            .decode()?
            .resize_exact(224, 224, image::imageops::FilterType::Lanczos3)
            .to_rgb8();

        let mut tensor: Vec<f32> = Vec::with_capacity(3 * 224 * 224);
        let mut r = Vec::with_capacity(224 * 224);
        let mut g = Vec::with_capacity(224 * 224);
        let mut b = Vec::with_capacity(224 * 224);
        for px in img.pixels() {
            r.push(px[0] as f32 / 255.0);
            g.push(px[1] as f32 / 255.0);
            b.push(px[2] as f32 / 255.0);
        }
        tensor.extend(r);
        tensor.extend(g);
        tensor.extend(b);

        // SigLIP normalisation stats — scale to [-1, 1].
        let mean = [0.5_f32, 0.5, 0.5];
        let std = [0.5_f32, 0.5, 0.5];
        for c in 0..3 {
            for i in 0..(224 * 224) {
                let idx = c * 224 * 224 + i;
                tensor[idx] = (tensor[idx] - mean[c]) / std[c];
            }
        }

        Ok(ndarray::Array4::from_shape_vec((1, 3, 224, 224), tensor)?)
    }
}

impl ImageEncoder for Siglip2ImageEncoder {
    #[tracing::instrument(name = "siglip2.encode_image", skip(self), fields(path = %image_path.display()))]
    fn encode(&mut self, image_path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_array = self.preprocess(image_path)?;
        let shape = [1usize, 3, 224, 224];
        let (data, _offset) = input_array.into_raw_vec_and_offset();
        let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;

        let outputs = self.session.run(ort::inputs![
            "pixel_values" => onnx_input
        ])?;

        // Try ranked output names. SigLIP exports vary.
        let extract = |name: &str| -> Option<Vec<f32>> {
            outputs.get(name).and_then(|t| {
                t.try_extract_tensor::<f32>()
                    .ok()
                    .map(|(_, view)| view.to_vec())
            })
        };
        let raw = extract("image_embeds")
            .or_else(|| extract("pooler_output"))
            .or_else(|| {
                extract("last_hidden_state").map(|v| {
                    let hidden = 768usize;
                    let seq = v.len() / hidden;
                    let mut pooled = vec![0.0_f32; hidden];
                    for i in 0..seq {
                        for j in 0..hidden {
                            pooled[j] += v[i * hidden + j];
                        }
                    }
                    for x in pooled.iter_mut() {
                        *x /= seq as f32;
                    }
                    pooled
                })
            })
            .ok_or("SigLIP-2 image: no recognised output name")?;

        debug!("SigLIP-2 image embedding length: {}", raw.len());
        Ok(normalize(&raw))
    }

    fn embedding_dim(&self) -> usize {
        768
    }

    fn id(&self) -> &'static str {
        "siglip2_base"
    }
}

// =====================================================================
// Text encoder
// =====================================================================

pub struct Siglip2TextEncoder {
    session: Session,
    tokenizer: Tokenizer,
    /// SigLIP-2's max sequence length is 64 tokens (per the model's
    /// config.json). Different from CLIP-multilingual's 128.
    max_seq_length: usize,
}

impl Siglip2TextEncoder {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initialising SigLIP-2 text encoder ===");
        info!("model: {}", model_path.display());
        info!("tokenizer: {}", tokenizer_path.display());

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("SigLIP-2 tokenizer load failed: {e}"))?;
        let session = Session::builder()?.commit_from_file(model_path)?;

        Ok(Self {
            session,
            tokenizer,
            max_seq_length: 64,
        })
    }

    /// Tokenise + pad/truncate to `max_seq_length`.
    fn encode_text(&self, text: &str) -> Result<Vec<i64>, Box<dyn Error>> {
        let encoded = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("SigLIP-2 tokenize failed: {e}"))?;
        let mut ids: Vec<i64> = encoded.get_ids().iter().map(|&i| i as i64).collect();

        // Pad with token id 1 (the SigLIP SentencePiece pad token id;
        // matches model's config.json pad_token_id). Truncate if too long.
        if ids.len() > self.max_seq_length {
            ids.truncate(self.max_seq_length);
        } else {
            ids.resize(self.max_seq_length, 1);
        }
        Ok(ids)
    }
}

impl TextEncoderTrait for Siglip2TextEncoder {
    #[tracing::instrument(name = "siglip2.encode_text", skip(self, text), fields(query_len = text.len()))]
    fn encode(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_ids = self.encode_text(text)?;
        let input_ids_tensor: Tensor<i64> =
            Tensor::from_array(([1usize, self.max_seq_length], input_ids))?;

        // SigLIP-2's text encoder takes only `input_ids` (no attention
        // mask in the standard ONNX export — sequence is fixed-length
        // padded). If your export expects attention_mask too, this will
        // surface as an ort error and you can add the input here.
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
        let raw = extract("text_embeds")
            .or_else(|| extract("pooler_output"))
            .or_else(|| extract("sentence_embedding"))
            .ok_or("SigLIP-2 text: no recognised output name")?;

        debug!("SigLIP-2 text embedding length: {}", raw.len());
        Ok(normalize(&raw))
    }

    fn embedding_dim(&self) -> usize {
        768
    }

    fn id(&self) -> &'static str {
        "siglip2_base"
    }
}

