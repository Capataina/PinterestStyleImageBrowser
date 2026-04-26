//! Encoder picker support — list available encoders + their metadata.
//!
//! Lets the frontend SettingsDrawer populate two dropdowns (text→image
//! encoder, image→image encoder) with options + descriptions for the
//! tooltip hover. Each entry tells the UI:
//!   - `id`: the encoder_id (matches embeddings.encoder_id column)
//!   - `display_name`: the user-facing label
//!   - `description`: ~one paragraph of "what to pick this for"
//!   - `dim`: output embedding dim (informational)
//!   - `supports_text`: whether this encoder family has a text branch
//!   - `supports_image`: whether this encoder family has an image branch
//!
//! Static list — encoders are compiled in. Adding an encoder means
//! editing this file + the matching Rust impl.

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct EncoderInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub dim: usize,
    pub supports_text: bool,
    pub supports_image: bool,
}

const ENCODERS: &[EncoderInfo] = &[
    EncoderInfo {
        id: "clip_vit_b_32",
        display_name: "CLIP ViT-B/32",
        description: "OpenAI's CLIP via Xenova's ONNX export. The multilingual text encoder supports 50+ languages but English quality is mediocre because it's a knowledge-distilled approximation. Currently the only working text→image encoder (SigLIP-2 ONNX exports were 401-gated when checked).",
        dim: 512,
        supports_text: true,
        supports_image: true,
    },
    EncoderInfo {
        id: "dinov2_small",
        display_name: "DINOv2 Small",
        description: "Meta's self-supervised image encoder. No text branch — image-only. Dominates CLIP at finding visually similar images: same person across photos, same character, similar pose, similar art style. 5× advantage on fine-grained image-image benchmarks. Recommended for the 'View Similar' feature.",
        dim: 384,
        supports_text: false,
        supports_image: true,
    },
    // SigLIP-2 entry removed for now — the Xenova/siglip2-base-patch16-224
    // and onnx-community/siglip2-base-patch16-224 URLs both 401'd when
    // checked. The encoder_siglip2.rs code is still there (image + text
    // encoders implementing the traits) so re-adding the entry is one
    // line once a verified ONNX export URL is found.
];

#[tauri::command]
#[tracing::instrument(name = "ipc.list_available_encoders")]
pub fn list_available_encoders() -> Vec<EncoderInfo> {
    ENCODERS.to_vec()
}
