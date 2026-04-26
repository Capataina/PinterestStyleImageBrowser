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
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
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

/// Process-global flag set once at startup from `--profile` on the
/// command line. Everything perf-related — the PerfLayer registration,
/// the frontend overlay mount, the user-action recorder, the on-exit
/// report writer — keys off this single source of truth.
///
/// Read via `is_profiling_enabled()`; written exactly once via
/// `set_profiling_enabled()` from `main.rs` before Tauri starts.
static PROFILING_ENABLED: OnceLock<bool> = OnceLock::new();

/// Mark this process as a profiling session. Idempotent — only the
/// first call wins, subsequent calls are silently ignored. Call once
/// at the very top of `main` before the tracing subscriber is built.
pub fn set_profiling_enabled(on: bool) {
    let _ = PROFILING_ENABLED.set(on);
}

/// True if this process was launched with `--profile`. Defaults to
/// false if the flag was never set (the normal app run).
pub fn is_profiling_enabled() -> bool {
    *PROFILING_ENABLED.get().unwrap_or(&false)
}

// =====================================================================
// Session + raw event log
// =====================================================================
//
// In addition to the per-span aggregates, profiling mode also records
// every span close and every user action as an individual `RawEvent`
// in a process-global ringbuffer. The buffer is drained to disk every
// `FLUSH_INTERVAL` by a background thread spawned at session start.
//
// This catches what aggregates can't: a single 5-second outlier among
// 99 fast calls barely moves the mean and only barely shows in p99,
// but the raw event is right there in the timeline — and crucially,
// correlated with whatever user action fired just before it.
//
// Design choices:
// - Ringbuffer cap of 50_000 events at ~80 bytes each = ~4 MB max in
//   memory. With a 5s flush interval that's ~10k events/sec headroom,
//   well above what we'd ever hit.
// - JSONL output (one event per line) so the file is append-only —
//   safe under SIGTERM, parseable line-by-line for streaming, easy to
//   diff between runs.
// - Timestamps are milliseconds since session start, not unix epoch:
//   small numbers, easy to read in the report, no risk of leaking
//   the user's wall clock if a report ever gets shared.

/// Maximum events buffered in memory before they MUST be flushed to
/// disk. If the flush thread falls behind, oldest events are dropped
/// (logged as a warning) — losing the trailing edge of a session is
/// preferable to OOMing.
const RAW_EVENT_CAP: usize = 50_000;

/// How often the background thread drains the raw event buffer to
/// `timeline.jsonl`. 5s balances "lose at most 5s on crash" against
/// "don't beat up the disk during heavy span traffic."
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// One row in the timeline. Three variants:
/// - `Span`: a tracing span closed (timing data).
/// - `User`: a user-action breadcrumb (e.g. clicked image, typed query).
/// - `Diagnostic`: rich structured snapshot of internal state — what
///   encoder is loaded, what cosine returned for a query, what an
///   embedding lookup hit/missed. The diagnostic framework feeds into
///   the on-exit report's "Diagnostics" section so the user can
///   correlate "search returned X results" with "actual top-N from
///   cosine was [...]" — pinpoints whether bad search results are an
///   encoding issue, a cosine issue, a DB-lookup issue, or a frontend
///   display issue.
///
/// `Deserialize` is implemented because the on-exit report renderer
/// reads the JSONL back line-by-line to build the markdown report,
/// and tests round-trip through it as a sanity check.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RawEvent {
    /// Emitted by `PerfLayer::on_close` for every instrumented span.
    Span {
        /// Milliseconds since session start. Same clock as User events
        /// so they sort into one timeline.
        ts_ms: u64,
        /// The span's metadata name (e.g. `clip.encode_image`).
        name: String,
        /// Wall-clock duration of the span in microseconds.
        duration_us: u64,
    },
    /// Emitted by the `record_user_action` Tauri command.
    User {
        ts_ms: u64,
        /// e.g. `search_submit`, `image_click`, `tag_toggle`.
        action: String,
        /// Whatever the call site decided to attach. Free-form JSON
        /// so we don't need a schema migration every time we add an
        /// instrumentation point.
        payload: Value,
    },
    /// Rich diagnostic snapshots of internal state. Emitted from
    /// backend code via `record_diagnostic`. Only fires when
    /// profiling is enabled.
    Diagnostic {
        ts_ms: u64,
        /// e.g. `startup_state`, `search_query`, `cosine_cache_populated`,
        /// `encoder_changed`, `embedding_lookup`, `path_resolution`.
        diagnostic: String,
        /// Free-form JSON. Each diagnostic kind has a documented
        /// expected shape (see `record_diagnostic` callers); the
        /// renderer uses the diagnostic field to pick the right
        /// formatter for the report.
        payload: Value,
    },
}

/// Process-global session start. Set once when profiling mode begins.
/// All RawEvent ts_ms values are computed relative to this instant.
static SESSION_START: OnceLock<Instant> = OnceLock::new();

/// Process-global session directory. Holds `timeline.jsonl` during
/// the session, plus `report.md` and `raw.json` written at exit.
static SESSION_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Process-global ringbuffer of pending raw events. Drained by the
/// background flush thread.
static RAW_EVENTS: OnceLock<Mutex<Vec<RawEvent>>> = OnceLock::new();

fn raw_events_buf() -> &'static Mutex<Vec<RawEvent>> {
    RAW_EVENTS.get_or_init(|| Mutex::new(Vec::with_capacity(1024)))
}

/// Initialise the profiling session. Creates the session directory
/// (`Library/exports/perf-{unix_ts}/`) and stores the start instant
/// so all subsequent events get relative timestamps. Returns the
/// session directory path so the caller can log it.
///
/// Safe to call only once per process. Subsequent calls are ignored
/// (the existing session continues).
pub fn init_session(exports_dir: PathBuf) -> io::Result<PathBuf> {
    let unix_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = exports_dir.join(format!("perf-{unix_ts}"));
    fs::create_dir_all(&dir)?;
    let _ = SESSION_START.set(Instant::now());
    let _ = SESSION_DIR.set(dir.clone());
    Ok(dir)
}

/// Path of the active session directory if profiling is enabled.
pub fn session_dir() -> Option<PathBuf> {
    SESSION_DIR.get().cloned()
}

/// Milliseconds since `init_session` was called. Returns 0 if the
/// session was never initialised (so events still get a timestamp,
/// they just all collapse to t=0).
fn session_ms() -> u64 {
    SESSION_START
        .get()
        .map(|s| s.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

/// Record a user action into the timeline. No-op if profiling is off,
/// so call sites don't need to gate themselves.
pub fn record_user_action(action: String, payload: Value) {
    if !is_profiling_enabled() {
        return;
    }
    push_event(RawEvent::User {
        ts_ms: session_ms(),
        action,
        payload,
    });
}

/// Record a diagnostic snapshot into the timeline. No-op when
/// profiling is off — call sites can sprinkle freely without
/// worrying about overhead. The on-exit report renders these into
/// a dedicated "Diagnostics" section grouped by `diagnostic` kind.
///
/// The standard diagnostic kinds and their expected payload shapes
/// are documented at each call site (see commands/similarity.rs,
/// commands/semantic.rs, lib.rs setup, etc.). New kinds can be added
/// freely — the renderer falls back to pretty-printing the payload
/// for unknown kinds.
pub fn record_diagnostic(diagnostic: &str, payload: Value) {
    if !is_profiling_enabled() {
        return;
    }
    push_event(RawEvent::Diagnostic {
        ts_ms: session_ms(),
        diagnostic: diagnostic.to_string(),
        payload,
    });
}

/// Append an event to the ringbuffer. If the buffer is at capacity
/// (flush thread fell behind), drop the oldest event and log a single
/// warning the first time it happens — we'd rather lose the trailing
/// tail than block the calling thread on disk I/O.
fn push_event(event: RawEvent) {
    let Ok(mut buf) = raw_events_buf().lock() else {
        return;
    };
    if buf.len() >= RAW_EVENT_CAP {
        // Drop oldest. Vec::remove(0) is O(n) but n is bounded at
        // RAW_EVENT_CAP and this only happens under saturation —
        // fine for diagnostics.
        buf.remove(0);
    }
    buf.push(event);
}

/// Spawn the background thread that drains the raw event buffer to
/// `timeline.jsonl` every `FLUSH_INTERVAL`. No-op if profiling is
/// off or the session directory wasn't initialised.
///
/// The thread runs until the process exits. Tauri kills it on app
/// shutdown along with everything else; we don't try to join it.
pub fn spawn_flush_thread() {
    if !is_profiling_enabled() {
        return;
    }
    let Some(dir) = session_dir() else {
        return;
    };
    let path = dir.join("timeline.jsonl");

    thread::spawn(move || loop {
        thread::sleep(FLUSH_INTERVAL);
        if let Err(e) = flush_to_file(&path) {
            // Don't crash the diagnostics on I/O hiccups — log and
            // try again on the next interval. If the disk is wedged,
            // the buffer will fill up and start dropping oldest
            // events; that's the right failure mode for telemetry.
            tracing::warn!("perf flush failed: {e}");
        }
    });
}

/// Drain the buffer and append every event as JSONL to `path`. Called
/// by the background thread; also called explicitly from the on-exit
/// report renderer to capture whatever's still in memory at shutdown.
pub fn flush_to_file(path: &std::path::Path) -> io::Result<()> {
    // Drain under the lock so we don't hold it during disk I/O.
    let drained: Vec<RawEvent> = {
        let mut buf = raw_events_buf().lock().map_err(|_| {
            io::Error::other("perf buffer mutex poisoned")
        })?;
        std::mem::take(&mut *buf)
    };
    if drained.is_empty() {
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for event in drained {
        // serde_json::to_string can only fail if the value contains
        // non-serialisable types — RawEvent is constructed from owned
        // Strings + serde_json::Value, both always serialisable.
        let line = serde_json::to_string(&event)
            .map_err(io::Error::other)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
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
            map.entry(name.clone()).or_default().record(elapsed_ns);
        }

        // Also append to the raw event log so the timeline view can
        // show this exact event (with its precise timestamp), not
        // just the aggregate. This is what catches single-event
        // spikes that aggregates would smooth over.
        push_event(RawEvent::Span {
            ts_ms: session_ms(),
            name,
            duration_us: elapsed_ns / 1000,
        });
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

    #[test]
    fn raw_event_span_serialises_with_kind_tag() {
        let e = RawEvent::Span {
            ts_ms: 1234,
            name: "clip.encode_image".into(),
            duration_us: 75_000,
        };
        let json = serde_json::to_string(&e).unwrap();
        // The internal #[serde(tag = "kind")] discriminator means the
        // JSON output has a "kind":"span" field — this is the contract
        // the report renderer relies on.
        assert!(json.contains("\"kind\":\"span\""));
        assert!(json.contains("\"ts_ms\":1234"));
        assert!(json.contains("\"name\":\"clip.encode_image\""));
        assert!(json.contains("\"duration_us\":75000"));
    }

    #[test]
    fn raw_event_user_serialises_with_payload_object() {
        let e = RawEvent::User {
            ts_ms: 5678,
            action: "image_click".into(),
            payload: serde_json::json!({ "id": 42 }),
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"kind\":\"user\""));
        assert!(json.contains("\"action\":\"image_click\""));
        assert!(json.contains("\"id\":42"));
    }

    #[test]
    fn flush_to_file_writes_jsonl_and_drains_buffer() {
        // Use a unique temp path so this test doesn't interact with
        // a real session directory left from another run.
        let dir = std::env::temp_dir().join(format!(
            "perf_flush_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("timeline.jsonl");

        // Push two events directly into the buffer (bypassing the
        // is_profiling_enabled check, which is process-global).
        push_event(RawEvent::Span {
            ts_ms: 100,
            name: "test.span".into(),
            duration_us: 50,
        });
        push_event(RawEvent::User {
            ts_ms: 200,
            action: "test_action".into(),
            payload: serde_json::json!({}),
        });

        flush_to_file(&path).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert!(
            lines.len() >= 2,
            "expected at least 2 lines (other tests may have pushed events), got {}: {written}",
            lines.len()
        );
        // Each line must be valid JSON on its own — that's the JSONL
        // contract the on-exit renderer reads.
        for line in &lines {
            let _: RawEvent = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("line is not valid RawEvent JSON: {line} ({e})"));
        }

        // Buffer should now be drained.
        let remaining = raw_events_buf().lock().unwrap().len();
        assert_eq!(
            remaining, 0,
            "flush should leave the buffer empty, found {remaining} events"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn flush_to_file_is_noop_when_buffer_empty() {
        // Drain anything pending from prior tests.
        let _ = raw_events_buf().lock().map(|mut b| b.clear());

        let dir = std::env::temp_dir().join(format!(
            "perf_flush_empty_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("timeline.jsonl");

        flush_to_file(&path).unwrap();
        // Empty buffer + empty flush ⇒ file is never created. That's
        // the contract: don't spam the disk with empty files every 5s
        // when the user is just sitting on the app idle.
        assert!(
            !path.exists(),
            "flush_to_file should not create the file when buffer is empty"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
