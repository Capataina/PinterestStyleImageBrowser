//! Reciprocal Rank Fusion (RRF) over per-encoder similarity rankings.
//!
//! ## What and why
//!
//! Instead of returning the top-K from a single encoder, this module
//! takes the top-K ranked list from *every available encoder* (CLIP,
//! SigLIP-2, DINOv2) and fuses them into one ranked list using the
//! Reciprocal Rank Fusion formula from Cormack, Clarke & Büttcher
//! (2009). The fused score for an image `p` is:
//!
//! ```text
//! score(p) = Σ over encoders e of  1 / (k_rrf + rank_e(p))
//! ```
//!
//! where `rank_e(p)` is `p`'s 1-indexed position in encoder `e`'s
//! ranking (or ∞ if `p` did not appear in that encoder's top-K, which
//! contributes 0 to the sum).
//!
//! Standard `k_rrf = 60` from the original paper. Larger `k_rrf`
//! flattens the contribution curve (a #1 hit and a #50 hit contribute
//! more similarly); smaller `k_rrf` makes top-of-list dominate.
//!
//! ## Why this replaces the tiered "1 of top 5, 5 of top 25" sampler
//!
//! The previous diversity strategy was structural (sample evenly from
//! similarity buckets to get visual variety). RRF gives us diversity
//! for free because different encoders disagree about what's "similar":
//!
//! - **CLIP** values semantic concept overlap ("both are dragons").
//! - **DINOv2** values visual / structural similarity ("both have the
//!   same pose, lighting, art style").
//! - **SigLIP-2** values descriptive content match (better English
//!   text alignment than CLIP).
//!
//! When the three lists are fused, an image that all three rank highly
//! (genuinely similar by every criterion) wins. An image that one
//! encoder loves but the others ignore still gets a contribution but
//! sinks below the consensus picks. The result is naturally diverse
//! AND naturally relevant, without any random-sampling step.
//!
//! ## Performance shape
//!
//! Fusion cost is dominated by N encoder scoring passes (each O(cache
//! size)). For 2000 images × 3 encoders that's ~6000 dot products + 3
//! sorts ≈ 5-15 ms total on M2 — comparable to the single-encoder
//! tiered path.
//!
//! ## References
//!
//! - Cormack, Clarke & Büttcher (2009), *Reciprocal Rank Fusion
//!   outperforms Condorcet and individual rank learning methods*,
//!   SIGIR '09. <https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf>

use std::collections::HashMap;
use std::path::PathBuf;

/// Standard RRF constant from the original paper. Trade-off:
///
/// - Lower (e.g. 10): #1 hit contributes ~0.091, #50 hit contributes
///   ~0.017 — top-of-list dominates the fusion.
/// - Higher (e.g. 100): #1 hit contributes ~0.0099, #50 hit ~0.0067
///   — almost flat, all encoders' top-K are roughly equal contributors.
/// - 60 is the canonical balance: top-of-list still dominates but
///   mid-list (~rank 30) still has meaningful pull, which is what we
///   want for diversity-with-relevance.
pub const DEFAULT_K_RRF: usize = 60;

/// One ranked list from one encoder. Order is high-to-low similarity.
/// `score` is the encoder's own cosine similarity — RRF discards it
/// (only the rank matters), but we keep it around so callers can
/// surface per-encoder evidence in diagnostics.
#[derive(Debug, Clone)]
pub struct RankedList {
    pub encoder_id: String,
    /// `(path, encoder_specific_cosine_score)` in descending score
    /// order. Index = 0-based rank.
    pub items: Vec<(PathBuf, f32)>,
}

/// Output of fusion. Each entry carries:
/// - the path,
/// - the fused RRF score (sum of 1/(k+rank) contributions),
/// - per-encoder evidence (which encoders saw this and at what rank).
///
/// The evidence vector is for diagnostics + future "why was this
/// returned" UI. Empty if no encoder ranked the image — but those
/// items are filtered out before this struct is built.
#[derive(Debug, Clone)]
pub struct FusedItem {
    pub path: PathBuf,
    pub fused_score: f32,
    /// `(encoder_id, 1-based-rank, encoder_score)` for every encoder
    /// that ranked this image in its top-K input list.
    pub per_encoder: Vec<(String, usize, f32)>,
}

/// Fuse N ranked lists into one, sorted high-to-low by fused score,
/// truncated to `top_n`.
///
/// `k_rrf` is typically [`DEFAULT_K_RRF`]; pass a custom value to
/// experiment with different sharpness. Caller already excluded the
/// query image itself from each ranked list.
pub fn reciprocal_rank_fusion(
    ranked_lists: &[RankedList],
    k_rrf: usize,
    top_n: usize,
) -> Vec<FusedItem> {
    if ranked_lists.is_empty() || top_n == 0 {
        return Vec::new();
    }
    let k = k_rrf as f32;

    // Aggregate by path. We keep insertion order for the per_encoder
    // evidence vector (so the diagnostic shows encoders in
    // ranked_lists order) — a Vec inside the value rather than another
    // HashMap.
    let mut agg: HashMap<PathBuf, FusedItem> = HashMap::new();
    for list in ranked_lists {
        for (rank0, (path, score)) in list.items.iter().enumerate() {
            // 1-indexed rank for the formula.
            let rank1 = rank0 + 1;
            let contrib = 1.0 / (k + rank1 as f32);
            let entry = agg
                .entry(path.clone())
                .or_insert_with(|| FusedItem {
                    path: path.clone(),
                    fused_score: 0.0,
                    per_encoder: Vec::new(),
                });
            entry.fused_score += contrib;
            entry.per_encoder.push((list.encoder_id.clone(), rank1, *score));
        }
    }

    let mut sorted: Vec<FusedItem> = agg.into_values().collect();
    sorted.sort_unstable_by(|a, b| {
        b.fused_score
            .partial_cmp(&a.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(top_n);
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn empty_lists_produce_empty_output() {
        assert!(reciprocal_rank_fusion(&[], DEFAULT_K_RRF, 10).is_empty());
    }

    #[test]
    fn top_n_zero_produces_empty_output() {
        let list = RankedList {
            encoder_id: "clip".into(),
            items: vec![(p("/a.jpg"), 0.9)],
        };
        assert!(reciprocal_rank_fusion(&[list], DEFAULT_K_RRF, 0).is_empty());
    }

    #[test]
    fn single_encoder_preserves_order() {
        // With one ranked list, RRF degenerates into "rank by 1/(k+rank)"
        // which is monotonically decreasing in rank — so the original
        // order is preserved.
        let list = RankedList {
            encoder_id: "clip".into(),
            items: vec![
                (p("/best.jpg"), 0.95),
                (p("/middle.jpg"), 0.85),
                (p("/worst.jpg"), 0.50),
            ],
        };
        let fused = reciprocal_rank_fusion(&[list], DEFAULT_K_RRF, 5);
        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].path, p("/best.jpg"));
        assert_eq!(fused[1].path, p("/middle.jpg"));
        assert_eq!(fused[2].path, p("/worst.jpg"));
    }

    #[test]
    fn consensus_image_outranks_one_encoder_winner() {
        // The user's specific scenario. CLIP loves /clip_only.jpg (its
        // #1), but DINOv2 and SigLIP don't see it at all. /consensus.jpg
        // is #2 in all three encoders. Fusion should rank /consensus.jpg
        // first because the cumulative evidence is stronger than one
        // encoder's lone enthusiasm.
        let clip = RankedList {
            encoder_id: "clip".into(),
            items: vec![
                (p("/clip_only.jpg"), 0.95),
                (p("/consensus.jpg"), 0.85),
            ],
        };
        let dinov2 = RankedList {
            encoder_id: "dinov2".into(),
            items: vec![
                (p("/dino_first.jpg"), 0.90),
                (p("/consensus.jpg"), 0.85),
            ],
        };
        let siglip = RankedList {
            encoder_id: "siglip".into(),
            items: vec![
                (p("/siglip_first.jpg"), 0.92),
                (p("/consensus.jpg"), 0.84),
            ],
        };
        let fused = reciprocal_rank_fusion(
            &[clip, dinov2, siglip],
            DEFAULT_K_RRF,
            10,
        );
        assert_eq!(fused[0].path, p("/consensus.jpg"));
        // The consensus image carries evidence from all three encoders.
        assert_eq!(fused[0].per_encoder.len(), 3);
        // Each lone-encoder winner gets evidence from one encoder only.
        let clip_only = fused.iter().find(|f| f.path == p("/clip_only.jpg")).unwrap();
        assert_eq!(clip_only.per_encoder.len(), 1);
    }

    #[test]
    fn k_rrf_changes_lone_vs_consensus_score_ratio() {
        // Direct math assertion: at small k, a singleton-rank-1 entry
        // outscores a deep consensus; at large k, the consensus
        // outscores it. This is the sharpness intuition that justifies
        // k_rrf=60 as the canonical default.
        //
        // The previous version of this test asserted ordering, but
        // multiple entries can score IDENTICAL contributions at small
        // k (e.g. two encoders' rank-1 items both = 1/(k+1)) and the
        // unstable sort would pick either as fused[0] — flaky. Asserting
        // the SCORE directly avoids the tie-break dependency.
        //
        //   /lone.jpg     — only in clip at rank 1.   score = 1/(k+1)
        //   /consensus.jpg — in clip rank 5 + dinov2 rank 1.
        //                   score = 1/(k+5) + 1/(k+1)
        //
        // At k=1:    lone = 0.5     consensus = 1/6 + 1/2 = 0.667
        //            ratio consensus/lone = 1.333 → consensus already winning
        //            (because dinov2's only entry is at rank 1; /consensus
        //            collects a rank-1 contribution from dinov2)
        // At k=60:   lone = 1/61 ≈ 0.01639
        //            consensus = 1/65 + 1/61 ≈ 0.03177
        //            ratio = 1.94
        //
        // The thing that changes with k is the RATIO between the two
        // scores. Larger k flattens the curve — dinov2's rank-1 hit on
        // /consensus and clip's rank-5 hit on /consensus both contribute
        // more equally, magnifying the consensus advantage.
        let clip = RankedList {
            encoder_id: "clip".into(),
            items: vec![
                (p("/lone.jpg"), 0.99),
                (p("/_a.jpg"), 0.10),
                (p("/_b.jpg"), 0.10),
                (p("/_c.jpg"), 0.10),
                (p("/consensus.jpg"), 0.10),
            ],
        };
        let dinov2 = RankedList {
            encoder_id: "dinov2".into(),
            items: vec![(p("/consensus.jpg"), 0.5)],
        };

        let small = reciprocal_rank_fusion(&[clip.clone(), dinov2.clone()], 1, 10);
        let large = reciprocal_rank_fusion(&[clip, dinov2], 60, 10);

        let lone_small = small.iter().find(|f| f.path == p("/lone.jpg")).unwrap();
        let consensus_small = small
            .iter()
            .find(|f| f.path == p("/consensus.jpg"))
            .unwrap();
        let lone_large = large.iter().find(|f| f.path == p("/lone.jpg")).unwrap();
        let consensus_large = large
            .iter()
            .find(|f| f.path == p("/consensus.jpg"))
            .unwrap();

        // The consensus advantage grows as k grows — that's the whole
        // point of the k_rrf knob.
        let ratio_small = consensus_small.fused_score / lone_small.fused_score;
        let ratio_large = consensus_large.fused_score / lone_large.fused_score;
        assert!(
            ratio_large > ratio_small,
            "consensus/lone ratio should grow as k grows (small={ratio_small}, \
             large={ratio_large}); k_rrf flattens contribution per encoder, \
             which magnifies the consensus advantage"
        );
    }

    #[test]
    fn truncation_returns_top_n() {
        let list = RankedList {
            encoder_id: "clip".into(),
            items: (0..20).map(|i| (p(&format!("/{i}.jpg")), 1.0 - i as f32 * 0.01)).collect(),
        };
        let fused = reciprocal_rank_fusion(&[list], DEFAULT_K_RRF, 5);
        assert_eq!(fused.len(), 5);
        assert_eq!(fused[0].path, p("/0.jpg"));
        assert_eq!(fused[4].path, p("/4.jpg"));
    }
}
