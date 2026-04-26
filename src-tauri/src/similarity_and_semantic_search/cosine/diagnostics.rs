//! Embedding-quality diagnostics for the profiling report.
//!
//! These functions compute statistics over a cache of embeddings —
//! L2 norms, per-dim distribution, NaN counts, pairwise distance
//! histogram, self-similarity check. The output is always a
//! `serde_json::Value` so callers can wrap it in a Diagnostic event
//! without any encoder-specific knowledge.
//!
//! Invoked from `populate_from_db_for_encoder` so every encoder swap
//! produces a fresh snapshot in the on-exit profiling report. Lets
//! the user answer questions like:
//!
//! - "Are these embeddings normalised?" (mean L2 norm)
//! - "Are any embeddings broken/empty?" (NaN count, min L2 norm)
//! - "Is this encoder discriminating between images?" (pairwise
//!   distribution — if all distances cluster around 0.7, the
//!   encoder isn't discriminating; if there's a wide spread, it is)
//! - "Is the encoder deterministic?" (self-similarity should be 1.0)

use ndarray::Array1;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::math::cosine_similarity;

/// Maximum number of embeddings to sample for the pairwise-distance
/// histogram. C(50, 2) = 1225 pair computations — fast even on CPU.
const PAIRWISE_SAMPLE_SIZE: usize = 50;

/// Compute embedding-quality stats for a populated cache.
///
/// Returns a JSON value with:
///   - count
///   - dim
///   - L2 norm summary (mean, min, max — should all be ~1.0 if the
///     encoder normalises)
///   - per-dim summary (mean of dim means, mean of dim stds — sanity
///     check that the embedding space isn't degenerate)
///   - NaN/Inf counts
///   - first 3 sample embeddings' first 8 dims (visual inspection)
///
/// Called once per encoder at populate time. ~few ms for a 1842
/// embedding library.
pub fn embedding_stats(cached_images: &[(PathBuf, Array1<f32>)]) -> Value {
    if cached_images.is_empty() {
        return json!({ "count": 0, "note": "cache empty — encoder has no embeddings" });
    }

    let count = cached_images.len();
    let dim = cached_images[0].1.len();

    // L2 norms — should be ~1.0 for normalised CLIP-family encoders.
    // If image encoder norms differ wildly from text encoder norms,
    // they're plausibly in different normalisations (a hint that they
    // might also be in different embedding spaces).
    let mut norms: Vec<f32> = Vec::with_capacity(count);
    let mut nan_count = 0usize;
    let mut inf_count = 0usize;
    for (_, emb) in cached_images {
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        norms.push(norm);
        for x in emb.iter() {
            if x.is_nan() {
                nan_count += 1;
            } else if x.is_infinite() {
                inf_count += 1;
            }
        }
    }
    let norm_mean = norms.iter().sum::<f32>() / count as f32;
    let norm_min = norms.iter().cloned().fold(f32::INFINITY, f32::min);
    let norm_max = norms.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Per-dim mean and std across all embeddings. Helps spot
    // degenerate spaces (e.g., all dims have mean ≈ 0 std ≈ 0 →
    // encoder is producing constant outputs).
    let mut dim_means: Vec<f32> = vec![0.0; dim];
    for (_, emb) in cached_images {
        for (j, x) in emb.iter().enumerate() {
            dim_means[j] += x;
        }
    }
    for d in dim_means.iter_mut() {
        *d /= count as f32;
    }
    let mut dim_vars: Vec<f32> = vec![0.0; dim];
    for (_, emb) in cached_images {
        for (j, x) in emb.iter().enumerate() {
            let diff = x - dim_means[j];
            dim_vars[j] += diff * diff;
        }
    }
    for v in dim_vars.iter_mut() {
        *v /= count as f32;
    }
    let mean_of_dim_means = dim_means.iter().sum::<f32>() / dim as f32;
    let mean_of_dim_stds = dim_vars.iter().map(|v| v.sqrt()).sum::<f32>() / dim as f32;

    // Sample embeddings for human inspection. First 3 images, first 8
    // dims of each. Lets the user eyeball "are these random-looking
    // numbers, or all zero, or all 0.5?"
    let samples: Vec<Value> = cached_images
        .iter()
        .take(3)
        .map(|(path, emb)| {
            let take = 8.min(emb.len());
            json!({
                "path": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                "first_8_dims": emb.iter().take(take).copied().collect::<Vec<f32>>(),
            })
        })
        .collect();

    json!({
        "count": count,
        "dim": dim,
        "l2_norm": {
            "mean": norm_mean,
            "min": norm_min,
            "max": norm_max,
            "interpretation": if (norm_mean - 1.0).abs() < 0.01 {
                "normalised (~1.0 — typical for CLIP/SigLIP/DINOv2 with explicit L2 normalize)"
            } else {
                "NOT normalised — might still work since cosine_similarity divides by norms, but worth flagging"
            },
        },
        "per_dim": {
            "mean_of_dim_means": mean_of_dim_means,
            "mean_of_dim_stds": mean_of_dim_stds,
        },
        "nan_count": nan_count,
        "inf_count": inf_count,
        "samples": samples,
    })
}

/// Compute pairwise cosine distribution from a sample of embeddings.
///
/// For up to PAIRWISE_SAMPLE_SIZE embeddings, computes cosine for
/// every pair — bins into 11 buckets covering [-0.1, 1.0] (negative
/// cosine bucket included for completeness; should be ~0 for
/// normalised embeddings).
///
/// The shape of this histogram is the most direct read on encoder
/// discrimination quality:
///   - All mass in [0.6, 0.8] → encoder isn't discriminating (every
///     image looks similar to every other)
///   - Wide spread [0.0, 1.0] → encoder discriminates well
///   - All mass in [0.95, 1.0] → likely a bug (every embedding
///     identical or near-identical)
pub fn pairwise_distance_distribution(
    cached_images: &[(PathBuf, Array1<f32>)],
) -> Value {
    if cached_images.len() < 2 {
        return json!({ "note": "need at least 2 embeddings for pairwise — skipping" });
    }
    let n = cached_images.len().min(PAIRWISE_SAMPLE_SIZE);
    // 11 buckets: [-1.0, -0.0] then [0.0, 0.1] [0.1, 0.2] ... [0.9, 1.0]
    let mut buckets: [u32; 11] = [0; 11];
    let mut pair_count = 0u32;
    let mut sum = 0.0f64;
    let mut min_seen = f32::INFINITY;
    let mut max_seen = f32::NEG_INFINITY;
    for i in 0..n {
        for j in (i + 1)..n {
            let s = cosine_similarity(&cached_images[i].1, &cached_images[j].1);
            pair_count += 1;
            sum += s as f64;
            min_seen = min_seen.min(s);
            max_seen = max_seen.max(s);
            // Bucket assignment.
            let b = if s < 0.0 {
                0
            } else if s >= 1.0 {
                10
            } else {
                ((s * 10.0) as usize) + 1
            };
            buckets[b.min(10)] += 1;
        }
    }
    let mean = if pair_count > 0 { sum / pair_count as f64 } else { 0.0 };

    let labels = [
        "[-, 0.0)",
        "[0.0, 0.1)",
        "[0.1, 0.2)",
        "[0.2, 0.3)",
        "[0.3, 0.4)",
        "[0.4, 0.5)",
        "[0.5, 0.6)",
        "[0.6, 0.7)",
        "[0.7, 0.8)",
        "[0.8, 0.9)",
        "[0.9, 1.0]",
    ];
    let histogram: Vec<Value> = labels
        .iter()
        .zip(buckets.iter())
        .map(|(l, c)| json!({ "bucket": l, "count": c }))
        .collect();

    json!({
        "sample_size": n,
        "pair_count": pair_count,
        "min": min_seen,
        "max": max_seen,
        "mean": mean,
        "histogram": histogram,
        "interpretation": "Wide spread = encoder discriminates well. \
                          All mass in [0.6, 0.8] = encoder isn't discriminating. \
                          All mass in [0.95, 1.0] = likely bug.",
    })
}

/// Self-similarity sanity check: cosine(emb, emb) should be exactly
/// 1.0 for any embedding. If it's not, something is fundamentally
/// wrong with either the cosine_similarity math or the embedding
/// itself (NaN, all zeros, etc.).
pub fn self_similarity_check(
    cached_images: &[(PathBuf, Array1<f32>)],
) -> Value {
    if cached_images.is_empty() {
        return json!({ "note": "cache empty — skipped" });
    }
    let (_, emb) = &cached_images[0];
    let s = cosine_similarity(emb, emb);
    json!({
        "embedding_norm": (emb.iter().map(|x| x * x).sum::<f32>()).sqrt(),
        "self_cosine": s,
        "expected": 1.0,
        "deviation": (1.0 - s).abs(),
        "passes": (1.0 - s).abs() < 1e-4,
    })
}

/// Compute summary statistics for a list of search-result scores.
/// Embedded in the search_query diagnostic so the report has a
/// quick read on "are these scores high or noise-floor".
pub fn score_distribution_stats(scores: &[f32]) -> Value {
    if scores.is_empty() {
        return json!({ "count": 0 });
    }
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let min = sorted[0];
    let max = sorted[n - 1];
    let mean = sorted.iter().sum::<f32>() / n as f32;
    let median = sorted[n / 2];
    let p95 = sorted[(n as f32 * 0.95) as usize];
    let variance = sorted.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n as f32;
    let std = variance.sqrt();
    json!({
        "count": n,
        "min": min,
        "max": max,
        "mean": mean,
        "median": median,
        "p95": p95,
        "std": std,
        "top1_vs_botN_gap": max - min,
        "interpretation": if std < 0.02 {
            "VERY low variance — likely noise floor (text/image misalignment, encoder broken, or all embeddings near-identical)"
        } else if std < 0.05 {
            "Low variance — weak signal; encoder may not be discriminating well on this corpus"
        } else if std < 0.15 {
            "Moderate variance — typical for well-aligned encoder on a homogeneous corpus"
        } else {
            "High variance — strong discrimination"
        },
    })
}
