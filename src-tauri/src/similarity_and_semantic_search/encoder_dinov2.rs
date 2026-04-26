//! DINOv2-Base image encoder.
//!
//! Self-supervised vision transformer from Meta FAIR (CVPR 2023).
//! Trained without text alignment — image-only, optimised for
//! image→image retrieval. DINOv2 dominates CLIP on image-image
//! similarity (~5× advantage on fine-grained 10k-class species
//! benchmarks).
//!
//! Has no text encoder — purpose is "View Similar" (image-clicked →
//! similar images). Text→image queries continue to use CLIP/SigLIP-2.
//!
//! ## Model — upgraded 2026-04-26 from -Small (384-d) to -Base (768-d)
//!
//! The Small variant has limited capacity for fine-grained
//! discrimination on a homogeneous corpus (e.g. anime art of one
//! character). Base doubles hidden width and quadruples parameter
//! count for materially better discrimination at ~4× inference cost.
//!
//! Sourced from `Xenova/dinov2-base`. Verified 2026-04-26 — 200 OK
//! at the URL below.
//!
//! ## Preprocessing — corrected 2026-04-26
//!
//! Now matches the canonical DINOv2 `BitImageProcessor` pipeline:
//!
//! 1. Convert to RGB
//! 2. Resize on shortest edge to 256 (bicubic / Catmull-Rom in
//!    image-rs, the closest match to PIL's BICUBIC)
//! 3. Center-crop to 224×224
//! 4. Rescale by 1/255 → [0, 1]
//! 5. Normalize with ImageNet mean=[0.485, 0.456, 0.406],
//!    std=[0.229, 0.224, 0.225]
//! 6. CHW layout → [1, 3, 224, 224] float32
//!
//! Previous code used `resize_exact(224, 224)` with Lanczos3, which
//! squashes non-square images (DINOv2 was trained with aspect-
//! preserving resize + center-crop). The canonical pipeline avoids
//! that distortion.
//!
//! ## Output
//!
//! `last_hidden_state` shape [1, 257, 768] — 256 patch tokens (224/14)²
//! plus the CLS token at index 0. We extract the CLS token. No
//! pooler_output is exported by Xenova's graph; the export terminates
//! at the final LayerNorm.

use image::ImageReader;
use image::imageops::FilterType;
use ort::{session::Session, value::Tensor};
use std::{error::Error, path::Path};
use tracing::{debug, info, warn};

use super::encoders::ImageEncoder;
use super::encoder_text::pooling::normalize;

pub const DINOV2_IMAGE_MODEL_URL: &str =
    "https://huggingface.co/Xenova/dinov2-base/resolve/main/onnx/model.onnx";

/// Filename has `_base_` to differentiate from any leftover
/// `model_dinov2_image.onnx` from the -Small build. Old file
/// coexists harmlessly until cleanup is added.
pub const DINOV2_IMAGE_MODEL_FILENAME: &str = "dinov2_base_image.onnx";

/// Stable encoder ID. Changed from `dinov2_small` so previously-
/// computed 384-d embeddings (under the `dinov2_small` key) don't
/// get fed into the 768-d cosine cache and crash the dim guard.
/// Existing 384-d rows are orphaned in the embeddings table — the
/// dim-mismatch invalidation pass will re-encode them as
/// `dinov2_base`.
pub const DINOV2_ENCODER_ID: &str = "dinov2_base";

const TARGET_SHORT_EDGE: u32 = 256;
const CROP: u32 = 224;
const HIDDEN: usize = 768;

pub struct Dinov2ImageEncoder {
    session: Session,
}

impl Dinov2ImageEncoder {
    pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initialising DINOv2-Base image encoder ===");
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
        // Load + decode + RGB.
        let img = ImageReader::open(image_path)?
            .with_guessed_format()?
            .decode()?
            .to_rgb8();

        let (orig_w, orig_h) = img.dimensions();
        // Aspect-preserving resize: scale shortest edge to TARGET_SHORT_EDGE.
        let (new_w, new_h) = if orig_w < orig_h {
            let new_w = TARGET_SHORT_EDGE;
            let new_h = (orig_h as f32 * (new_w as f32 / orig_w as f32)).round() as u32;
            (new_w, new_h)
        } else {
            let new_h = TARGET_SHORT_EDGE;
            let new_w = (orig_w as f32 * (new_h as f32 / orig_h as f32)).round() as u32;
            (new_w, new_h)
        };
        // CatmullRom is image-rs's bicubic-family filter — closest
        // match to PIL.Image.BICUBIC (resample=3). Visually equivalent
        // for embedding similarity; not bit-exact but standard for
        // ONNX deployments outside Python.
        let resized = image::imageops::resize(&img, new_w, new_h, FilterType::CatmullRom);

        // Center-crop to CROP × CROP.
        let crop_x = (new_w.saturating_sub(CROP)) / 2;
        let crop_y = (new_h.saturating_sub(CROP)) / 2;
        let cropped = image::imageops::crop_imm(&resized, crop_x, crop_y, CROP, CROP).to_image();

        let mut tensor: Vec<f32> = Vec::with_capacity(3 * (CROP as usize) * (CROP as usize));
        let mut r = Vec::with_capacity((CROP * CROP) as usize);
        let mut g = Vec::with_capacity((CROP * CROP) as usize);
        let mut b = Vec::with_capacity((CROP * CROP) as usize);
        for px in cropped.pixels() {
            r.push(px[0] as f32 / 255.0);
            g.push(px[1] as f32 / 255.0);
            b.push(px[2] as f32 / 255.0);
        }
        tensor.extend(r);
        tensor.extend(g);
        tensor.extend(b);

        // ImageNet normalisation — DINOv2's training stats. Distinct
        // from CLIP-stats and SigLIP-stats; each encoder must be fed
        // the distribution it was trained on or embedding quality
        // degrades silently.
        let mean = [0.485_f32, 0.456, 0.406];
        let std = [0.229_f32, 0.224, 0.225];
        let plane = (CROP * CROP) as usize;
        for c in 0..3 {
            for i in 0..plane {
                let idx = c * plane + i;
                tensor[idx] = (tensor[idx] - mean[c]) / std[c];
            }
        }

        Ok(ndarray::Array4::from_shape_vec(
            (1, 3, CROP as usize, CROP as usize),
            tensor,
        )?)
    }
}

impl ImageEncoder for Dinov2ImageEncoder {
    #[tracing::instrument(name = "dinov2.encode_image", skip(self), fields(path = %image_path.display()))]
    fn encode(&mut self, image_path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
        let input_array = self.preprocess(image_path)?;
        let shape = [1usize, 3, CROP as usize, CROP as usize];
        let (data, _offset) = input_array.into_raw_vec_and_offset();
        let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;

        let outputs = self.session.run(ort::inputs![
            "pixel_values" => onnx_input
        ])?;

        // Xenova's DINOv2-Base export emits `last_hidden_state`
        // [1, 257, 768] — 1 CLS token + 256 patches at 14×14 patch
        // size on a 224×224 input. We slice the CLS token (first row).
        // No `pooler_output` is exported.
        let raw = if let Some(t) = outputs.get("last_hidden_state") {
            let (shape, view) = t.try_extract_tensor::<f32>()?;
            let data = view.to_vec();
            let hidden = if shape.len() == 3 {
                shape[2] as usize
            } else {
                HIDDEN
            };
            // CLS token = first row in the sequence dimension.
            data[..hidden].to_vec()
        } else if let Some(t) = outputs.get("pooler_output") {
            // Defensive fallback if a future export adds this output.
            let (_, view) = t.try_extract_tensor::<f32>()?;
            view.to_vec()
        } else {
            return Err("DINOv2 output extraction: no recognised output name".into());
        };

        debug!("DINOv2 raw embedding length: {}", raw.len());
        Ok(normalize(&raw))
    }

    fn embedding_dim(&self) -> usize {
        // DINOv2-Base = 768-dim. (Small was 384, Large is 1024.)
        HIDDEN
    }

    fn id(&self) -> &'static str {
        DINOV2_ENCODER_ID
    }
}
