//! DINOv2-Small image encoder.
//!
//! Self-supervised vision transformer from Meta FAIR (CVPR 2023).
//! Trained without text alignment — image-only, optimised for
//! image→image retrieval. Per the project-enhancement agent's
//! research, DINOv2 dominates CLIP on image-image similarity:
//! 64% vs 28% on challenging dataset; 70% vs 15% on fine-grained
//! 10k-class species (5× advantage).
//!
//! Has no text encoder — purpose is "View Similar" (image-clicked →
//! similar images). Text→image queries continue to use CLIP/SigLIP.
//!
//! ## Preprocessing
//!
//! - Resize to 224×224 (Lanczos3).
//! - Normalise with ImageNet stats — mean [0.485, 0.456, 0.406],
//!   std [0.229, 0.224, 0.225]. (Different from BOTH CLIP-stats and
//!   SigLIP-stats; that's why per-encoder preprocessing matters.)
//! - CHW layout for ONNX.
//!
//! ## Output
//!
//! DINOv2-Small produces a 384-dim CLS-token embedding. The standard
//! ONNX export emits this as `last_hidden_state` shape [1, N, 384]
//! where N is the number of patches+1 (CLS first); we extract the
//! CLS token (index 0) as the image embedding.
//!
//! ## Model URL
//!
//! Sourced from `Xenova/dinov2-small`. If 404s, check the current
//! `Xenova` HuggingFace exports.

use image::ImageReader;
use ort::{session::Session, value::Tensor};
use std::{error::Error, path::Path};
use tracing::{debug, info, warn};

use super::encoders::ImageEncoder;
use super::encoder_text::pooling::normalize;

pub const DINOV2_IMAGE_MODEL_URL: &str =
    "https://huggingface.co/Xenova/dinov2-small/resolve/main/onnx/model.onnx";

pub const DINOV2_IMAGE_MODEL_FILENAME: &str = "model_dinov2_image.onnx";

pub struct Dinov2ImageEncoder {
    session: Session,
}

impl Dinov2ImageEncoder {
    pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initialising DINOv2-Small image encoder ===");
        info!("model: {}", model_path.display());

        let session = match Session::builder()?.commit_from_file(model_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("DINOv2 image session init failed ({e}); cannot load encoder");
                return Err(e.into());
            }
        };
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

        // ImageNet normalisation — DINOv2's training stats. Distinct
        // from CLIP-stats (used by ClipImageEncoder) and SigLIP-stats
        // (used by Siglip2ImageEncoder). Each encoder has its own
        // distribution to maintain alignment with how it was trained.
        let mean = [0.485_f32, 0.456, 0.406];
        let std = [0.229_f32, 0.224, 0.225];
        for c in 0..3 {
            for i in 0..(224 * 224) {
                let idx = c * 224 * 224 + i;
                tensor[idx] = (tensor[idx] - mean[c]) / std[c];
            }
        }

        Ok(ndarray::Array4::from_shape_vec((1, 3, 224, 224), tensor)?)
    }
}

impl ImageEncoder for Dinov2ImageEncoder {
    #[tracing::instrument(name = "dinov2.encode_image", skip(self), fields(path = %image_path.display()))]
    fn encode(&mut self, image_path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_array = self.preprocess(image_path)?;
        let shape = [1usize, 3, 224, 224];
        let (data, _offset) = input_array.into_raw_vec_and_offset();
        let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;

        let outputs = self.session.run(ort::inputs![
            "pixel_values" => onnx_input
        ])?;

        // DINOv2 ONNX exports emit `last_hidden_state` shape [1, N, 384]
        // where N is patches+1 (CLS token first). We extract the CLS
        // token at index 0. Some exports also emit `pooler_output` which
        // is a pre-pooled embedding — try that first since it's cheaper.
        let raw = if let Some(t) = outputs.get("pooler_output") {
            let (_, view) = t.try_extract_tensor::<f32>()?;
            view.to_vec()
        } else if let Some(t) = outputs.get("last_hidden_state") {
            let (shape, view) = t.try_extract_tensor::<f32>()?;
            let data = view.to_vec();
            // shape[1] = patches+1; shape[2] = hidden dim (384)
            let hidden = if shape.len() == 3 {
                shape[2] as usize
            } else {
                384
            };
            // CLS token = first row in the sequence dimension
            data[..hidden].to_vec()
        } else {
            return Err("DINOv2 output extraction: no recognised output name".into());
        };

        debug!("DINOv2 raw embedding length: {}", raw.len());
        Ok(normalize(&raw))
    }

    fn embedding_dim(&self) -> usize {
        // DINOv2-Small uses 384-dim hidden. DINOv2-Base = 768; Large = 1024.
        384
    }

    fn id(&self) -> &'static str {
        "dinov2_small"
    }
}
