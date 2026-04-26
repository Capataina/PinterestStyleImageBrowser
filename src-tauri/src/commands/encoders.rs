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
        display_name: "CLIP ViT-B/32 (OpenAI)",
        description: "OpenAI's English-only CLIP via Xenova's ONNX export. Both text and image branches in the same 512-dim space (separate vision_model.onnx + text_model.onnx). The reliable baseline for both text-to-image and image-to-image search. Lower-quality text alignment than SigLIP-2 but extremely battle-tested.",
        dim: 512,
        supports_text: true,
        supports_image: true,
    },
    EncoderInfo {
        id: "siglip2_base",
        display_name: "SigLIP-2 Base 256",
        description: "Google's modern sigmoid-loss CLIP successor (ICCV 2025). Both text and image branches in a shared 768-dim space. Better English text-to-image alignment than CLIP, especially for descriptive queries. Uses Gemma SentencePiece tokenizer (256k vocab). Recommended for the 'Semantic Search' text query feature. Image branch is also strong; pick this if you want one encoder for both directions.",
        dim: 768,
        supports_text: true,
        supports_image: true,
    },
    EncoderInfo {
        id: "dinov2_base",
        display_name: "DINOv2 Base",
        description: "Meta's self-supervised image encoder (768-dim, upgraded from the previous 384-dim Small variant). No text branch — image-only. Dominates CLIP/SigLIP at finding visually similar images: same person across photos, same character, similar pose, similar art style. Recommended for the 'View Similar' (image-clicked) feature where text queries don't apply.",
        dim: 768,
        supports_text: false,
        supports_image: true,
    },
];

#[tauri::command]
#[tracing::instrument(name = "ipc.list_available_encoders")]
pub fn list_available_encoders() -> Vec<EncoderInfo> {
    ENCODERS.to_vec()
}

/// Persist the user's image-encoder pick so the indexing pipeline can
/// run that encoder first and hot-populate the cosine cache for it as
/// soon as its phase finishes. Without this, the pipeline always runs
/// CLIP → SigLIP-2 → DINOv2 in fixed order — picking DINOv2 means
/// waiting for the other two to finish before yours starts.
///
/// Stored in `settings.json` under `priority_image_encoder`. The
/// indexing pipeline reads it at the start of each spawn, so a change
/// here only takes effect on the next pipeline run (e.g. add_root,
/// app launch, or watcher rescan).
///
/// Validation: the id must match one of the encoders in `ENCODERS`.
/// Unknown ids return BadInput so the frontend can surface the error
/// rather than silently writing a value the pipeline will ignore.
#[tauri::command]
#[tracing::instrument(name = "ipc.set_priority_image_encoder", skip())]
pub fn set_priority_image_encoder(id: String) -> Result<(), super::ApiError> {
    if !ENCODERS.iter().any(|e| e.id == id && e.supports_image) {
        return Err(super::ApiError::BadInput(format!(
            "Unknown image encoder id '{id}' — not in the available encoders list"
        )));
    }
    let mut s = crate::settings::Settings::load();
    s.priority_image_encoder = Some(id);
    s.save()
        .map_err(|e| super::ApiError::Internal(format!("settings save failed: {e}")))?;
    Ok(())
}
