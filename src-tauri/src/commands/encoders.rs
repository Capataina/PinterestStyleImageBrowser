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
/// Validate `id` against the compiled encoder list. Extracted so the
/// command body and the unit tests share one definition of "is this a
/// pickable image encoder" — adding a fourth encoder shouldn't require
/// touching multiple call sites.
fn is_known_image_encoder(id: &str) -> bool {
    ENCODERS.iter().any(|e| e.id == id && e.supports_image)
}

/// Pure decision function for the `set_priority_image_encoder` command —
/// extracted from the command body so it's testable without filesystem
/// isolation games (`paths::settings_path()` resolves to a process-wide
/// location, so two parallel cargo tests sharing it would race).
///
/// Returns:
///   - `Err(BadInput)` if `id` isn't a known image encoder,
///   - `Ok(None)` if the value already matches what's persisted (the
///     idempotent short-circuit — caller skips the disk write),
///   - `Ok(Some(id))` if a write is needed; caller persists the value.
///
/// The on-exit profiling report at t=5.87s showed
/// `set_priority_image_encoder` firing twice per drawer open. The
/// frontend `useRef` dedup is the primary fix; this short-circuit is
/// defence-in-depth so any other caller (current or future) doesn't
/// produce settings.json churn — every save was paired with an
/// unnecessary fsync on what was already the persisted value.
fn decide_priority_write(
    current: Option<&str>,
    requested: &str,
) -> Result<Option<String>, super::ApiError> {
    if !is_known_image_encoder(requested) {
        return Err(super::ApiError::BadInput(format!(
            "Unknown image encoder id '{requested}' — not in the available encoders list"
        )));
    }
    if current == Some(requested) {
        return Ok(None);
    }
    Ok(Some(requested.to_string()))
}

#[tauri::command]
#[tracing::instrument(name = "ipc.set_priority_image_encoder", skip())]
pub fn set_priority_image_encoder(id: String) -> Result<(), super::ApiError> {
    let mut s = crate::settings::Settings::load();
    match decide_priority_write(s.priority_image_encoder.as_deref(), &id)? {
        None => Ok(()), // already at requested value — no disk write
        Some(next) => {
            s.priority_image_encoder = Some(next);
            s.save().map_err(|e| {
                super::ApiError::Internal(format!("settings save failed: {e}"))
            })?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_write_rejects_unknown_encoder() {
        // BadInput is the same shape the original command produced —
        // frontend surfaces this rather than silently writing a value
        // the indexing pipeline would ignore.
        let err = decide_priority_write(None, "not_a_real_encoder").unwrap_err();
        match err {
            super::super::ApiError::BadInput(msg) => {
                assert!(msg.contains("not_a_real_encoder"));
            }
            other => panic!("expected BadInput, got {other:?}"),
        }
    }

    #[test]
    fn decide_write_short_circuits_when_value_matches() {
        // Regression test for the on-exit profiling-report finding at
        // t=5.87s: `set_priority_image_encoder` fired twice per drawer
        // open, each call doing a full settings.json round-trip even
        // though the value never changed. The frontend now dedupes via
        // useRef, this asserts the backend also no-ops on a same-value
        // push so any future caller can't reintroduce the churn.
        let result = decide_priority_write(Some("dinov2_base"), "dinov2_base").unwrap();
        assert_eq!(
            result, None,
            "matching current value must short-circuit (no Some(_)) so the caller skips the disk write"
        );
    }

    #[test]
    fn decide_write_proceeds_when_value_changes() {
        // The dedup must not silently drop a genuine encoder change —
        // that would leave settings.json out of sync with the picker
        // and the next pipeline run would use the old priority.
        let result = decide_priority_write(Some("clip_vit_b_32"), "dinov2_base").unwrap();
        assert_eq!(result, Some("dinov2_base".to_string()));
    }

    #[test]
    fn decide_write_proceeds_when_no_prior_value() {
        // First-ever set: settings.json has no priority_image_encoder
        // field yet (None), the user picks one in the drawer. Must
        // produce a write so the pipeline picks it up next run.
        let result = decide_priority_write(None, "dinov2_base").unwrap();
        assert_eq!(result, Some("dinov2_base".to_string()));
    }
}
