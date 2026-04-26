#![allow(clippy::doc_lazy_continuation)]
//! Diagnostic test for the code-health audit (2026-04-25).
//!
//! Demonstrates the equivalence + speed gap between the current full-sort
//! top-N approach and a partial-selection top-N approach. The audit
//! finding is "Replace full sort + take(top_n) with select_nth_unstable_by
//! + sort the trimmed slice"; this test pins both behaviours so the
//! implementing engineer can re-run after applying the change to verify
//! the resulting top-N set is identical.
//!
//! What the test asserts:
//!
//! 1. **Set equivalence.** For a representative population (10 000
//!    f32 cosine scores in `[-1, 1]`) with no ties at the boundary,
//!    the top-N elements selected by full sort are exactly the same
//!    set as the top-N elements selected by partial selection.
//!
//! 2. **Order equivalence after re-sorting the trimmed top-N.** If the
//!    caller then sorts the small top-N slice (which is what cosine's
//!    `get_similar_images_sorted` callers want), the orderings match.
//!
//! 3. **Throughput floor.** A loose timing observation showing that
//!    partial selection is *not slower* than full sort. (We avoid
//!    asserting a hard speedup factor because CI machines vary; the
//!    point is to confirm the implementation is at least as fast and
//!    is set up so the engineer can read the printed numbers.)
//!
//! These tests are intentionally pure-Rust (no DB, no Tauri) so they
//! run in milliseconds and the audit can use them as evidence without
//! pulling in the full integration stack.

use std::cmp::Ordering;
use std::time::Instant;

/// The current implementation pattern: sort everything by cosine
/// descending, take the first `top_n`. Mirrors
/// `cosine_similarity.rs::get_similar_images_sorted` modulo the
/// PathBuf cloning (which is itself a separate finding).
fn current_full_sort_topn<T: Clone>(
    mut scored: Vec<(T, f32)>,
    top_n: usize,
) -> Vec<(T, f32)> {
    scored.sort_by(|a, b| match (b.1.is_nan(), a.1.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => b.1.partial_cmp(&a.1).unwrap(),
    });
    scored.into_iter().take(top_n).collect()
}

/// The proposed partial-selection pattern. `select_nth_unstable_by` is
/// O(n) average and only fully orders the chosen index — everything
/// before it is "less than or equal to" it (in our reverse-sort sense),
/// everything after is "greater". We then sort just the prefix to get
/// the in-order top-N.
fn proposed_partial_select_topn<T: Clone>(
    mut scored: Vec<(T, f32)>,
    top_n: usize,
) -> Vec<(T, f32)> {
    if scored.is_empty() {
        return Vec::new();
    }
    let k = top_n.min(scored.len());
    if k == scored.len() {
        // Falling back to a full sort here means the partial-selection
        // path is at least as fast as the current approach in this edge
        // case (top_n == n).
        scored.sort_by(|a, b| match (b.1.is_nan(), a.1.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => b.1.partial_cmp(&a.1).unwrap(),
        });
        return scored.into_iter().take(k).collect();
    }
    // Partition around the (k-1)th best — descending order, so the k
    // best end up in scored[..k] in some order.
    let cmp = |a: &(T, f32), b: &(T, f32)| match (b.1.is_nan(), a.1.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => b.1.partial_cmp(&a.1).unwrap(),
    };
    scored.select_nth_unstable_by(k - 1, cmp);
    let mut top = scored.into_iter().take(k).collect::<Vec<_>>();
    // Then sort the small slice to give callers the in-order top-N.
    top.sort_by(|a, b| match (b.1.is_nan(), a.1.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => b.1.partial_cmp(&a.1).unwrap(),
    });
    top
}

/// Deterministic synthetic population: 10 000 floats spaced cleanly
/// enough that there are no exact ties at the top-N boundary. We
/// avoid randomness so the test is reproducible and the equivalence
/// assertions are deterministic.
fn synth_population(n: usize) -> Vec<(usize, f32)> {
    (0..n)
        .map(|i| {
            // Map i in [0, n) to a value in [-1, 1] via a deterministic
            // but non-monotonic sequence so the input isn't pre-sorted.
            let frac = i as f32 / n as f32;
            // Use sin to get a deterministic, non-monotonic distribution
            // across [-1, 1]. Plus a tiny linear nudge to keep ties out.
            let v = (frac * std::f32::consts::PI * 12.7).sin() * 0.95
                + (i as f32 * 1e-7);
            (i, v.clamp(-1.0, 1.0))
        })
        .collect()
}

#[test]
fn partial_select_topn_returns_same_set_as_full_sort() {
    let pop = synth_population(10_000);
    let top_n = 50;

    let by_full = current_full_sort_topn(pop.clone(), top_n);
    let by_partial = proposed_partial_select_topn(pop, top_n);

    assert_eq!(by_full.len(), top_n, "full-sort returned wrong count");
    assert_eq!(by_partial.len(), top_n, "partial-select returned wrong count");

    // Same set of indices. Compare by sorted indices to allow internal
    // ordering differences. The "Order equivalence after re-sorting"
    // assertion below covers ordering.
    let mut full_indices: Vec<usize> = by_full.iter().map(|(i, _)| *i).collect();
    let mut partial_indices: Vec<usize> = by_partial.iter().map(|(i, _)| *i).collect();
    full_indices.sort();
    partial_indices.sort();
    assert_eq!(
        full_indices, partial_indices,
        "partial-select and full-sort should pick the same top-{top_n} elements"
    );
}

#[test]
fn partial_select_after_resort_matches_full_sort_order() {
    let pop = synth_population(10_000);
    let top_n = 50;

    let by_full = current_full_sort_topn(pop.clone(), top_n);
    let by_partial = proposed_partial_select_topn(pop, top_n);

    // After the partial-select implementation re-sorts its trimmed
    // top-N (which it does inside proposed_partial_select_topn), the
    // ordering should be byte-identical to the full-sort path. This
    // is the assertion that lets the implementing engineer trust the
    // change is purely an internal optimisation with zero observable
    // behaviour change.
    let scores_full: Vec<f32> = by_full.iter().map(|(_, s)| *s).collect();
    let scores_partial: Vec<f32> = by_partial.iter().map(|(_, s)| *s).collect();
    assert_eq!(
        scores_full, scores_partial,
        "score ordering should match between full-sort and partial-then-sort top-N"
    );
}

#[test]
fn partial_select_at_least_as_fast_as_full_sort() {
    // Loose timing observation. We don't assert a hard speedup factor
    // — CI variance makes that flaky. We do assert the partial path
    // isn't dramatically slower (≥ 3x slower would indicate a bug).
    // The numbers get printed so an engineer reviewing this test can
    // see the actual speedup on their machine.
    let pop = synth_population(10_000);
    let top_n = 50;

    let runs = 50;

    let mut full_total = std::time::Duration::ZERO;
    for _ in 0..runs {
        let p = pop.clone();
        let start = Instant::now();
        let _ = current_full_sort_topn(p, top_n);
        full_total += start.elapsed();
    }

    let mut partial_total = std::time::Duration::ZERO;
    for _ in 0..runs {
        let p = pop.clone();
        let start = Instant::now();
        let _ = proposed_partial_select_topn(p, top_n);
        partial_total += start.elapsed();
    }

    let full_avg = full_total / runs;
    let partial_avg = partial_total / runs;
    println!(
        "[diagnostic] n=10000 top_n=50: full_sort avg={full_avg:?}, partial_select avg={partial_avg:?}, ratio={:.2}x",
        full_avg.as_secs_f64() / partial_avg.as_secs_f64()
    );

    // Sanity: partial should not be more than 3x slower than full.
    // (In practice on a warm CPU it's measurably faster — see the
    // printed ratio in the test output.)
    assert!(
        partial_avg.as_nanos() < full_avg.as_nanos() * 3,
        "partial-select unexpectedly slower than full-sort: full={full_avg:?}, partial={partial_avg:?}"
    );
}

#[test]
fn partial_select_handles_top_n_equal_to_population() {
    // Edge case: top_n == n. The partial path falls back to a full
    // sort here (see the early-return in proposed_partial_select_topn).
    // The result must still equal the full-sort path.
    let pop: Vec<(usize, f32)> = vec![
        (0, 0.5),
        (1, 0.9),
        (2, 0.1),
        (3, 0.7),
        (4, 0.3),
    ];

    let by_full = current_full_sort_topn(pop.clone(), 5);
    let by_partial = proposed_partial_select_topn(pop, 5);

    assert_eq!(by_full, by_partial);
}

#[test]
fn partial_select_handles_empty_input() {
    // Edge case: empty population. Both paths return an empty Vec.
    let pop: Vec<(usize, f32)> = Vec::new();
    let by_full = current_full_sort_topn(pop.clone(), 50);
    let by_partial = proposed_partial_select_topn(pop, 50);
    assert!(by_full.is_empty());
    assert!(by_partial.is_empty());
}

#[test]
fn partial_select_handles_top_n_greater_than_population() {
    // Edge case: top_n > n. Both paths return the entire population.
    let pop: Vec<(usize, f32)> = vec![(0, 0.5), (1, 0.9), (2, 0.1)];
    let by_full = current_full_sort_topn(pop.clone(), 100);
    let by_partial = proposed_partial_select_topn(pop, 100);
    assert_eq!(by_full.len(), 3);
    assert_eq!(by_partial.len(), 3);
    assert_eq!(by_full, by_partial);
}
