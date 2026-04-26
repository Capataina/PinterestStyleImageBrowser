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

/// Validate `id` against the compiled encoder list — used by the
/// per-encoder enable/disable validation. Adding a 4th encoder
/// requires only updating the static `ENCODERS` array; this helper
/// stays as-is.
fn is_known_encoder(id: &str) -> bool {
    ENCODERS.iter().any(|e| e.id == id)
}

/// Phase 11c — pure decision function for `set_enabled_encoders`.
///
/// Returns:
///   - `Err(BadInput)` if any requested id isn't in `ENCODERS`,
///   - `Err(BadInput)` if the resulting list would be empty (we never
///     allow zero encoders — that silently bricks every search),
///   - `Ok(None)` if the requested set already matches what's
///     persisted (idempotent short-circuit; caller skips the disk
///     write to avoid settings.json churn),
///   - `Ok(Some(deduped_sorted))` if a write is needed.
///
/// The dedup-and-sort makes equality comparison stable regardless of
/// frontend ordering — `["clip", "dino"]` and `["dino", "clip"]`
/// hash-equal under this function so the dedup doesn't fight the
/// user.
// `pub` (not `pub(crate)`) so audit-generated diagnostic tests under
// `src-tauri/tests/` can reference the validator directly. The function
// is otherwise an implementation detail of `set_enabled_encoders` —
// callers should still go through the IPC.
pub fn decide_enabled_write(
    current: Option<&[String]>,
    requested: &[String],
) -> Result<Option<Vec<String>>, super::ApiError> {
    let mut deduped: Vec<String> = Vec::new();
    for id in requested {
        if !is_known_encoder(id) {
            return Err(super::ApiError::BadInput(format!(
                "Unknown encoder id '{id}' — not in the available encoders list"
            )));
        }
        if !deduped.iter().any(|d| d == id) {
            deduped.push(id.clone());
        }
    }
    deduped.sort();

    if deduped.is_empty() {
        return Err(super::ApiError::BadInput(
            "Cannot disable every encoder — at least one must be enabled".into(),
        ));
    }

    let current_normalised: Option<Vec<String>> = current.map(|c| {
        let mut v = c.to_vec();
        v.sort();
        v.dedup();
        v
    });
    if current_normalised.as_deref() == Some(deduped.as_slice()) {
        return Ok(None);
    }
    Ok(Some(deduped))
}

/// Read the persisted enabled-encoder list. Returns the resolved list
/// (falls back to the default set if the user hasn't set a
/// preference). Frontend uses this to populate the toggle states on
/// drawer mount.
#[tauri::command]
#[tracing::instrument(name = "ipc.get_enabled_encoders")]
pub fn get_enabled_encoders() -> Vec<String> {
    crate::settings::Settings::load().resolved_enabled_encoders()
}

/// Persist the per-encoder enable/disable list. Frontend calls this
/// when the user toggles any encoder switch in the Settings drawer.
///
/// The new value takes effect immediately for fusion (the next
/// `get_fused_similar_images` / `get_fused_semantic_search` call
/// reads from this list) and on the next indexing pipeline run for
/// encoding (re-enabled encoders only re-encode the rows they don't
/// already have rows for).
#[tauri::command]
#[tracing::instrument(name = "ipc.set_enabled_encoders", skip())]
pub fn set_enabled_encoders(ids: Vec<String>) -> Result<(), super::ApiError> {
    let mut s = crate::settings::Settings::load();
    let current = s.enabled_encoders.as_deref();
    match decide_enabled_write(current, &ids)? {
        None => Ok(()),
        Some(next) => {
            s.enabled_encoders = Some(next);
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
    fn decide_enabled_rejects_unknown_id() {
        let err = decide_enabled_write(None, &["not_a_real_encoder".into()]).unwrap_err();
        match err {
            super::super::ApiError::BadInput(msg) => {
                assert!(msg.contains("not_a_real_encoder"));
            }
            other => panic!("expected BadInput, got {other:?}"),
        }
    }

    #[test]
    fn decide_enabled_rejects_empty_list() {
        // Disabling every encoder would silently break every fusion
        // call. Reject at the IPC boundary so the user gets a
        // surface-level error and the toggle bounces back.
        let err = decide_enabled_write(Some(&["clip_vit_b_32".into()]), &[]).unwrap_err();
        match err {
            super::super::ApiError::BadInput(msg) => {
                assert!(msg.contains("at least one"));
            }
            other => panic!("expected BadInput, got {other:?}"),
        }
    }

    #[test]
    fn decide_enabled_short_circuits_on_set_equality() {
        // Same set, different order — must produce no write.
        let result = decide_enabled_write(
            Some(&["dinov2_base".into(), "clip_vit_b_32".into()]),
            &["clip_vit_b_32".into(), "dinov2_base".into()],
        )
        .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn decide_enabled_proceeds_on_set_change() {
        let result = decide_enabled_write(
            Some(&["clip_vit_b_32".into()]),
            &["clip_vit_b_32".into(), "dinov2_base".into()],
        )
        .unwrap();
        assert_eq!(
            result,
            Some(vec!["clip_vit_b_32".to_string(), "dinov2_base".to_string()])
        );
    }

    #[test]
    fn decide_enabled_dedupes_input() {
        // Frontend toggle quirks shouldn't end up persisting
        // `["clip", "clip"]`.
        let result =
            decide_enabled_write(None, &["clip_vit_b_32".into(), "clip_vit_b_32".into()])
                .unwrap();
        assert_eq!(result, Some(vec!["clip_vit_b_32".to_string()]));
    }
}
