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
        display_name: "CLIP ViT-B/32 (legacy)",
        description: "OpenAI's original CLIP from 2021. Decent generalist; the multilingual text encoder supports 50+ languages but English quality is mediocre because it's a knowledge-distilled approximation. Superseded by SigLIP-2 on benchmarks but still works fine and is the default fallback.",
        dim: 512,
        supports_text: true,
        supports_image: true,
    },
    EncoderInfo {
        id: "siglip2_base",
        display_name: "SigLIP-2 Base (recommended for text→image)",
        description: "Google's 2025 update to CLIP with sigmoid loss. Better text→image alignment than CLIP-multilingual on English content per ICCV/ICLR papers. Pair the SigLIP-2 text + image encoders together for best results. Uses SentencePiece tokenisation.",
        dim: 768,
        supports_text: true,
        supports_image: true,
    },
    EncoderInfo {
        id: "dinov2_small",
        display_name: "DINOv2 Small (recommended for image→image)",
        description: "Meta's self-supervised image encoder. No text branch — image-only. Dominates CLIP/SigLIP at finding visually similar images: same person across photos, same character, similar pose, similar art style. 5× advantage on fine-grained image-image benchmarks. Best choice for the 'View Similar' feature.",
        dim: 384,
        supports_text: false,
        supports_image: true,
    },
];

#[tauri::command]
#[tracing::instrument(name = "ipc.list_available_encoders")]
pub fn list_available_encoders() -> Vec<EncoderInfo> {
    ENCODERS.to_vec()
}
