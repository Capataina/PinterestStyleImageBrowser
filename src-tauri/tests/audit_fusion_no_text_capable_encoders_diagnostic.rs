//! Audit diagnostic for `commands/semantic_fused.rs` empty-result
//! branch.
//!
//! Documented in `context/plans/code-health-audit/area-2-fusion-and-search.md` § K-FUS-1.
//!
//! `get_fused_semantic_search` returns `Ok(Vec::new())` when no enabled
//! encoder is text-capable (DINOv2 is image-only). The audit flagged
//! this as a UX collision: the empty result is indistinguishable from
//! "query matched no images." This test documents the contract via
//! the only public surface available to an integration test — the
//! `Settings::resolved_enabled_encoders` helper that
//! `get_fused_semantic_search` consults.
//!
//! `decide_enabled_write` (the IPC validator) is `pub(crate)` and not
//! reachable from an integration test, so we exercise the post-resolve
//! path instead: the audit's claim is that
//! `resolved_enabled_encoders()` can return a list whose intersection
//! with `["clip_vit_b_32", "siglip2_base"]` (the text-capable set) is
//! empty — i.e. a DINOv2-only configuration is permitted at the
//! settings layer.
//!
//! Marked `#[ignore]` because it documents a behaviour the audit
//! recommends changing; running it as part of CI would lock in the
//! current shape.

use image_browser_lib::settings::Settings;

const TEXT_CAPABLE: &[&str] = &["clip_vit_b_32", "siglip2_base"];

#[test]
#[ignore = "audit diagnostic — documents that a text-only-disable configuration is permitted at the settings layer"]
fn dinov2_only_settings_resolves_with_zero_text_capable_encoders() {
    // Construct a Settings with only DINOv2 enabled. The
    // resolved_enabled_encoders helper passes this through unchanged
    // (it strips empties + falls back when the list is fully empty,
    // but it does NOT enforce text-capability).
    let s = Settings {
        scan_root: None,
        priority_image_encoder: None,
        enabled_encoders: Some(vec!["dinov2_base".to_string()]),
    };
    let resolved = s.resolved_enabled_encoders();
    assert_eq!(resolved, vec!["dinov2_base".to_string()]);

    // The intersection with TEXT_CAPABLE is empty. This is what
    // get_fused_semantic_search sees, and the audit's K-FUS-1 finding
    // documents that the IPC currently returns Ok(Vec::new()) in this
    // case rather than a typed error.
    let intersection: Vec<&str> = TEXT_CAPABLE
        .iter()
        .copied()
        .filter(|tc| resolved.iter().any(|e| e == tc))
        .collect();
    assert!(
        intersection.is_empty(),
        "DINOv2-only config has no text-capable encoder; \
         get_fused_semantic_search currently returns Ok-empty"
    );
}

#[test]
#[ignore = "audit diagnostic — confirms an empty enabled list falls back to the default (which IS text-capable)"]
fn empty_enabled_list_falls_back_to_default_with_text_capable_encoders() {
    // Companion test: the empty-list fallback is the safety net that
    // protects users who manage to clear their toggle list. It returns
    // the default (CLIP + SigLIP-2 + DINOv2), which intersects
    // TEXT_CAPABLE non-trivially.
    let s = Settings {
        scan_root: None,
        priority_image_encoder: None,
        enabled_encoders: Some(vec![]),
    };
    let resolved = s.resolved_enabled_encoders();
    let intersection: Vec<&str> = TEXT_CAPABLE
        .iter()
        .copied()
        .filter(|tc| resolved.iter().any(|e| e == tc))
        .collect();
    assert!(!intersection.is_empty(), "default set must include text encoders");
}
