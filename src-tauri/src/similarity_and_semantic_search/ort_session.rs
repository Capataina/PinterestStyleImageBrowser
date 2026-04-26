//! Shared ort `Session` builder, tuned for the M2 hybrid CPU.
//!
//! All four encoders (CLIP image + text, DINOv2-Base, SigLIP-2 image
//! + text) construct their `Session` through this helper instead of
//! `Session::builder()?.commit_from_file(path)`. Centralising means
//! every model gets the same M2-correct thread-pool sizing and graph
//! optimisation level, and a future tuning change lands in one place.
//!
//! ## Why these specific knobs
//!
//! `with_intra_threads(4)` — the M2 has 4 performance + 4 efficiency
//! cores. ORT's default thread auto-detect picks the full 8, which is
//! actively harmful for latency-bound inference: pinning work to the
//! mixed cluster collapses the P-cores' max frequency to the E-cores'
//! frequency (Apple's hybrid scheduler matches frequency across an
//! active cluster). 4 keeps every active core on the P-cluster at full
//! frequency. This is documented in [ONNX Runtime threading
//! docs](https://onnxruntime.ai/docs/performance/tune-performance/threading.html)
//! and confirmed by the m2-perf-options research at
//! `context/references/m2-perf-options-2026-04.md` § A7.
//!
//! `with_inter_threads(1)` — we batch sequentially, not as parallel
//! sub-graphs. inter_threads > 1 only helps graphs with independent
//! parallel branches; transformers don't have many.
//!
//! `with_optimization_level(Level3)` — the most aggressive of ORT's
//! built-in graph rewrites: constant folding, common-subexpression
//! elimination, layer fusion (BiasAdd into Conv, etc.). Always-on for
//! us because our models are loaded once and inferred millions of
//! times — every per-inference saving compounds.
//!
//! ## Why no `dynamic_block_base`
//!
//! The perf plan mentions `dynamic_block_base=4` as part of the R4
//! bundle. The pyke/ort 2.0-rc.10 Rust binding does not expose it as a
//! safe Rust method (it's available via raw `set_session_config_entry`
//! string keys, but that's a brittle path that bypasses type-checking).
//! Skipping it costs us a small additional speedup on dynamic shapes;
//! Level3 + intra_threads(4) is the load-bearing pair.
//!
//! ## Pre-warming
//!
//! Each encoder has its own pre-warm path because the input shape
//! differs (CLIP/DINOv2: 1×3×224×224 f32; SigLIP-2: 1×3×256×256 f32;
//! text encoders: i64 token ids). The helper here only builds the
//! session — pre-warming lives in each encoder's constructor so the
//! exact input tensor shape is right.

use ort::{
    session::{Session, builder::GraphOptimizationLevel},
};
use std::path::Path;
use tracing::info;

/// Build a `Session` from an ONNX model on disk with M2-tuned thread
/// + optimisation settings.
///
/// `label` is included in the log line so a future perf report can
/// confirm every encoder went through this path (vs an old
/// `Session::builder()?.commit_from_file()` site that was missed).
pub fn build_tuned_session(
    label: &str,
    model_path: &Path,
) -> Result<Session, ort::Error> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .with_inter_threads(1)?
        .commit_from_file(model_path)?;
    info!(
        "ort session ready ({label}, Level3, intra=4, inter=1, path={})",
        model_path.display()
    );
    Ok(session)
}
