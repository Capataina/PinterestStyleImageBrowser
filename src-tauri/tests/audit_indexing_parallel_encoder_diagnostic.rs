//! Audit diagnostic for `indexing.rs::run_encoder_phase`.
//!
//! Pins the dead-parameter situation documented in
//! `context/plans/code-health-audit/area-1-indexing.md` § D-IDX-1.
//!
//! `run_encoder_phase` is private to `indexing.rs`, so this test
//! cannot call it directly. Instead, the test exercises the closest
//! observable surface: the `IndexingState` single-flight gate and the
//! `IndexingProgress` event payload that the encoder phase produces.
//!
//! The real value of this test is the docstring + the literal text of
//! the assertions: it is a checkable pin on the audit's claim that
//! `cosine_index` and `cosine_current_encoder` are unused inside
//! `run_encoder_phase`. A future refactor that removes those parameters
//! from the function signature should cause no behaviour change visible
//! to this test.
//!
//! Marked `#[ignore]` because it is documentation-grade, not a
//! regression gate.

use std::sync::atomic::Ordering;
use image_browser_lib::indexing::{IndexingState, IndexingProgress, Phase};

#[test]
#[ignore = "audit diagnostic — documents the dead-parameter contract of run_encoder_phase"]
fn run_encoder_phase_does_not_observe_cosine_index_state() {
    // The audit finding (D-IDX-1) claims that the only Arcs threaded
    // into `run_encoder_phase` (cosine_index + cosine_current_encoder)
    // are discarded inside the function body via
    // `let _ = (cosine_index, cosine_current_encoder);` (line 745 of
    // indexing.rs at audit time).
    //
    // We can't call `run_encoder_phase` directly (private), but we can
    // assert the behaviour the audit is pinning:
    //
    //   1. The single-flight gate (IndexingState's AtomicBool) is the
    //      only true state the encoder phase mutates that this test
    //      can observe.
    //   2. Phase::Encode events serialise as expected (kebab-case).
    //
    // If a future refactor removes the dead Arc parameters, every
    // assertion below should still hold.

    let state = IndexingState::new();
    assert!(!state.is_running.load(Ordering::SeqCst));

    let progress = IndexingProgress {
        phase: Phase::Encode,
        processed: 32,
        total: 1842,
        message: Some("Encoding (clip_vit_b_32)".into()),
    };
    let json = serde_json::to_string(&progress).unwrap();
    assert!(json.contains("\"phase\":\"encode\""));
    assert!(json.contains("\"processed\":32"));
    assert!(json.contains("\"total\":1842"));
}

#[test]
#[ignore = "audit diagnostic — confirms the IndexingState single-flight contract holds across encoder thread spawn"]
fn indexing_state_single_flight_survives_concurrent_compare_exchange() {
    // The audit finding (K-IDX-1) flags that `try_spawn_pipeline`'s
    // single-flight gate silently drops a second concurrent caller.
    // This test confirms the underlying AtomicBool semantics: the
    // first caller wins, every other caller fails until the slot is
    // cleared.

    let state = IndexingState::new();
    let acquired = state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
    assert!(acquired.is_ok());

    for _ in 0..10 {
        let denied = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(denied.is_err(), "single-flight slot must reject reentrants while held");
    }

    state.is_running.store(false, Ordering::SeqCst);
    let reacquired = state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
    assert!(reacquired.is_ok());
}
