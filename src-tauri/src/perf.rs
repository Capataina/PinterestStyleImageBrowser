//! Performance diagnostics collector.
//!
//! A `tracing-subscriber::Layer` that intercepts span enter/exit
//! events, accumulates per-span-name statistics in process memory,
//! and exposes the aggregate to the frontend via a Tauri command.
//!
//! Why not HdrHistogram or proper percentile tracking up front?
//! Because the simplest thing that works (count, sum, min, max, plus
//! a fixed-size ringbuffer of recent samples) catches every regression
//! that matters for a desktop app, without dragging in 3 KB of
//! configuration per histogram. The ringbuffer lets us compute p50/p95
//! on demand from the last N samples, and we can graduate to
//! HdrHistogram if we ever genuinely need 5+ significant digits at
//! 60-second percentile windows.
//!
//! Each span writes one sample on close. Spans nested inside other
//! spans count independently (no roll-up). The overhead per span is
//! a few hundred nanoseconds — negligible for operations that take
//! microseconds or more, which is what we care about.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Maximum number of recent samples kept per span, for on-demand
/// percentile calculation. 200 samples × 8 bytes per u64 = 1.6 KB
/// per span × ~50 distinct spans = ~80 KB peak. Trivial.
const RECENT_SAMPLES_CAP: usize = 200;

#[derive(Default)]
struct SpanStats {
    count: u64,
    /// Total elapsed time across every recorded sample, in nanoseconds.
    /// Sufficient for `mean = total_ns / count`.
    total_ns: u64,
    /// Min and max from any sample we've ever seen.
    min_ns: u64,
    max_ns: u64,
    /// Ringbuffer of the most recent samples (in nanoseconds). Used
    /// to compute approximate p50/p95 on demand without keeping every
    /// sample forever.
    recent: Vec<u64>,
}

impl SpanStats {
    fn record(&mut self, ns: u64) {
        if self.count == 0 {
            self.min_ns = ns;
            self.max_ns = ns;
        } else {
            self.min_ns = self.min_ns.min(ns);
            self.max_ns = self.max_ns.max(ns);
        }
        self.count += 1;
        self.total_ns = self.total_ns.saturating_add(ns);

        if self.recent.len() == RECENT_SAMPLES_CAP {
            // Drop oldest. Vec-based ringbuffer is O(n) on remove(0)
            // but n is 200 — negligible. A VecDeque would be cleaner
            // but Vec is fine for our scale.
            self.recent.remove(0);
        }
        self.recent.push(ns);
    }
}

/// Process-global perf collector. Lazy-initialised because we need
/// the tracing subscriber to register first.
static PERF_STATS: OnceLock<Mutex<HashMap<String, SpanStats>>> = OnceLock::new();

fn stats_map() -> &'static Mutex<HashMap<String, SpanStats>> {
    PERF_STATS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `tracing` Layer that records every span's enter/exit and writes a
/// duration sample on span close.
pub struct PerfLayer;

impl PerfLayer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PerfLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-span scratch state — just the wall-clock instant the span
/// was entered. Stored in tracing's per-span extension storage so we
/// don't need a side table keyed by span id.
struct EnterTime(Instant);

impl<S> Layer<S> for PerfLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        _attrs: &Attributes<'_>,
        id: &Id,
        ctx: Context<'_, S>,
    ) {
        // Stamp creation time so the first `on_enter` doesn't have to
        // worry about it. Some tracing patterns enter a span multiple
        // times; we treat the first enter as the start.
        if let Some(span) = ctx.span(id) {
            let mut ext = span.extensions_mut();
            if ext.get_mut::<EnterTime>().is_none() {
                ext.insert(EnterTime(Instant::now()));
            }
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        // If the span has been entered for the first time without
        // on_new_span firing first (rare), still stamp it.
        if let Some(span) = ctx.span(id) {
            let mut ext = span.extensions_mut();
            if ext.get_mut::<EnterTime>().is_none() {
                ext.insert(EnterTime(Instant::now()));
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let ext = span.extensions();
        let Some(start) = ext.get::<EnterTime>() else { return };
        let elapsed_ns = start.0.elapsed().as_nanos() as u64;

        let name = span.metadata().name().to_string();

        if let Ok(mut map) = stats_map().lock() {
            map.entry(name).or_default().record(elapsed_ns);
        }
    }

    fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
        // No-op for events. We only care about spans for timing.
    }
}

/// Public snapshot of one span's accumulated stats. Sent to the
/// frontend over Tauri IPC.
#[derive(Debug, Serialize, Clone)]
pub struct SpanSnapshot {
    pub name: String,
    pub count: u64,
    pub mean_us: f64,
    pub min_us: f64,
    pub max_us: f64,
    /// p50 from the most recent up to `RECENT_SAMPLES_CAP` samples.
    pub p50_us: f64,
    /// p95 from the same recent window.
    pub p95_us: f64,
    /// p99 from the same recent window. Less reliable when count is low.
    pub p99_us: f64,
    /// How many samples the percentiles were computed from.
    pub recent_window: usize,
}

/// Top-level perf snapshot returned to the frontend.
#[derive(Debug, Serialize, Clone)]
pub struct PerfSnapshot {
    pub spans: Vec<SpanSnapshot>,
    /// Wall-clock when the snapshot was taken (unix epoch seconds).
    pub timestamp: i64,
}

/// Compute a percentile from a sorted-in-place clone of the samples.
fn percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    if pct <= 0.0 {
        return sorted[0];
    }
    if pct >= 100.0 {
        return sorted[sorted.len() - 1];
    }
    let idx = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Build a snapshot of every span's current stats. Sorts the recent
/// ringbuffer in place to compute percentiles, then leaves the
/// stored data unchanged (we work on a clone).
pub fn snapshot() -> PerfSnapshot {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let map = match stats_map().lock() {
        Ok(m) => m,
        Err(p) => p.into_inner(), // recover from poison; don't panic during diagnostics
    };

    let mut spans: Vec<SpanSnapshot> = map
        .iter()
        .map(|(name, stats)| {
            let mut recent = stats.recent.clone();
            recent.sort_unstable();
            SpanSnapshot {
                name: name.clone(),
                count: stats.count,
                mean_us: if stats.count > 0 {
                    (stats.total_ns as f64) / (stats.count as f64) / 1000.0
                } else {
                    0.0
                },
                min_us: stats.min_ns as f64 / 1000.0,
                max_us: stats.max_ns as f64 / 1000.0,
                p50_us: percentile(&recent, 50.0) as f64 / 1000.0,
                p95_us: percentile(&recent, 95.0) as f64 / 1000.0,
                p99_us: percentile(&recent, 99.0) as f64 / 1000.0,
                recent_window: recent.len(),
            }
        })
        .collect();

    // Sort by mean duration descending — slowest things at the top
    // of any UI rendering this snapshot.
    spans.sort_by(|a, b| {
        b.mean_us
            .partial_cmp(&a.mean_us)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    PerfSnapshot { spans, timestamp }
}

/// Wipe all collected stats. Useful between scenarios when the user
/// wants to measure a specific operation in isolation.
pub fn reset() {
    if let Ok(mut map) = stats_map().lock() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_stats_records_min_max_correctly() {
        let mut s = SpanStats::default();
        s.record(100);
        s.record(50);
        s.record(200);
        assert_eq!(s.min_ns, 50);
        assert_eq!(s.max_ns, 200);
        assert_eq!(s.count, 3);
        assert_eq!(s.total_ns, 350);
    }

    #[test]
    fn span_stats_ringbuffer_caps_at_max() {
        let mut s = SpanStats::default();
        for i in 0..(RECENT_SAMPLES_CAP + 50) {
            s.record(i as u64);
        }
        assert_eq!(s.recent.len(), RECENT_SAMPLES_CAP);
        assert_eq!(s.count, (RECENT_SAMPLES_CAP + 50) as u64);
        // Oldest 50 should have been dropped; the remaining buffer
        // should start at sample index 50.
        assert_eq!(s.recent[0], 50);
        assert_eq!(s.recent[RECENT_SAMPLES_CAP - 1], (RECENT_SAMPLES_CAP + 49) as u64);
    }

    #[test]
    fn percentile_handles_edges() {
        let sorted = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(percentile(&sorted, 0.0), 1);
        assert_eq!(percentile(&sorted, 100.0), 10);
        // p50 of 1..10 should land in the middle.
        let p50 = percentile(&sorted, 50.0);
        assert!(p50 == 5 || p50 == 6);
        // Empty input returns 0.
        assert_eq!(percentile(&[], 50.0), 0);
    }

    #[test]
    fn snapshot_returns_empty_when_no_spans_recorded() {
        // Reset first so this test doesn't pick up samples from
        // other tests in the same process.
        reset();
        let snap = snapshot();
        assert!(snap.spans.is_empty());
        assert!(snap.timestamp >= 0);
    }
}
