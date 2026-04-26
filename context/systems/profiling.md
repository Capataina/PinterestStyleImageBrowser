# profiling

*Maturity: comprehensive*

## Scope / Purpose

Opt-in performance diagnostics. When the binary is launched with the `--profiling` CLI flag (named `--profiling`, NOT `--profile`, because Tauri 2's CLI has its own `--profile <NAME>` for cargo profile selection — they collide) OR with the `PROFILING=1` env var set, every meaningful operation (Tauri commands, indexing phases, model downloads, watcher events, cosine retrievals, DB methods) emits `tracing` spans that are aggregated by an in-process `PerfLayer`, written to a JSONL timeline on disk, correlated with user-action breadcrumbs from the frontend, and rendered into a markdown report on app exit. The frontend mirrors the flag with a `<PerfOverlay>` panel (cmd+shift+P) and a `perfInvoke` wrapper that emits per-IPC start/end events.

When `--profiling` is absent, every code path described here is dormant. The PerfLayer never registers, the overlay never mounts, action breadcrumbs never call into the backend, and the on-exit renderer is a no-op. The only cost is a single `tracing` dispatch per instrumented call (the env filter passes the spans, but no aggregator builds them), which is in the few-hundred-nanosecond range — invisible for everything except the absolute hottest microsecond-level paths.

## Boundaries / Ownership

- **Owns:** `--profiling` flag parsing (`main.rs`), `PROFILING_ENABLED` and `PERF_STATS` `OnceLock`s, the `PerfLayer` `tracing-subscriber::Layer` impl, `SpanStats` aggregation (count/sum/min/max/recent ringbuffer for percentile estimation), the `RawEvent` log + JSONL flush thread, `record_user_action` IPC + correlation window, the on-exit `report.md`/`raw.json` renderer, the frontend `<PerfOverlay>` UI, `services/perf.ts` wrappers, the `perfInvoke` IPC wrapper, the `cmd+shift+P` shortcut, the <app_data_dir>/exports/perf-<unix_ts>/ session directory layout.
- **Does not own:** the spans themselves (every system has its own `#[tracing::instrument]`), the underlying `tracing-subscriber` registry (provided by main.rs), the JSON schema for any specific span (those live with their owning systems).
- **Public API (Rust):** `perf::is_profiling_enabled()`, `perf::set_profiling_enabled(bool)`, `perf::PerfLayer::new()`, `perf::session_dir() -> Option<PathBuf>`, `perf::init_session(exports_dir)`, `perf::spawn_flush_thread()`, `perf::flush_to_file(path)`, `perf::snapshot() -> PerfSnapshot`, `perf::reset()`, `perf::record_user_action(action, payload)`, `perf_report::render_session_report(session_dir)`.
- **Public API (Tauri commands):** `is_profiling_enabled`, `get_perf_snapshot`, `reset_perf_stats`, `export_perf_snapshot`, `record_user_action`.
- **Public API (frontend):** `isProfilingEnabled()`, `getPerfSnapshot()`, `recordAction(name, payload)`, `exportPerfSnapshot()`, `perfInvoke(cmd, args)` (wraps Tauri `invoke` with start/end events), `onRenderProfiler` (React.Profiler callback), `<PerfOverlay open={...} onClose={...} />`.

## Current Implemented Reality

### Activation flow

```
main.rs:
  let profiling = std::env::args().any(|a| a == "--profiling")
      || std::env::var("PROFILING").map(|v| !v.is_empty() && v != "0").unwrap_or(false);
  perf::set_profiling_enabled(profiling);

  // Build subscriber stack with envfilter
  let registry = tracing_subscriber::registry()
      .with(env_filter)
      .with(fmt::layer().with_target(true));

  if profiling {
      let _ = registry.with(perf::PerfLayer::new()).try_init();
      perf::init_session(paths::exports_dir())?;  // creates <app_data_dir>/exports/perf-{ts}/
      perf::spawn_flush_thread();                 // every 5s drains RawEvent log to timeline.jsonl
  } else {
      let _ = registry.try_init();                // no PerfLayer, no session dir
  }
```

### `PerfLayer` (the aggregator)

A `tracing-subscriber::Layer` that intercepts span enter/exit events and accumulates per-name stats:

```rust
struct SpanStats {
    count: u64,
    total_ns: u64,                  // for mean = total/count
    min_ns: u64,
    max_ns: u64,
    recent: Vec<u64>,               // ringbuffer (cap 200) for on-demand p50/p95
}
```

Stored in a process-global `OnceLock<Mutex<HashMap<String, SpanStats>>>`. Each span writes one sample on close. Spans nested inside other spans count independently (no roll-up). Overhead per span is a few hundred nanoseconds — negligible for operations the project cares about (microseconds or more).

`RECENT_SAMPLES_CAP = 200` × 8 bytes × ~50 distinct spans ≈ 80 KB peak memory. Trivial.

### Timeline log + JSONL flush

Every span close also pushes a `RawEvent` onto an in-memory log; a background thread spawned via `perf::spawn_flush_thread()` drains the log every 5 seconds and appends the events to `<app_data_dir>/exports/perf-{unix_ts}/timeline.jsonl`. User-action breadcrumbs from the frontend (`record_user_action` IPC) write to the same log so on-exit correlation can attribute span events to the user action that triggered them.

The 500 ms `CORRELATION_WINDOW_MS` in `perf_report.rs` is the threshold for "this span event was caused by this user action" — anything later than that, the user has already stopped attributing the lag.

### On-exit report

`tauri::RunEvent::Exit` handler in `lib.rs::run`:

```rust
.run(|_app, event| {
    if let tauri::RunEvent::Exit = event {
        if perf::is_profiling_enabled() {
            if let Some(dir) = perf::session_dir() {
                match perf_report::render_session_report(&dir) {
                    Ok(_) => eprintln!("profiling report written to {}", dir.display()),
                    Err(e) => eprintln!("failed to write profiling report: {e}"),
                }
            }
        }
    }
});
```

`render_session_report` produces three artefacts in the session dir:

| File | Format | Purpose |
|------|--------|---------|
| `timeline.jsonl` | One `RawEvent` per line | Raw event stream for custom analysis |
| `report.md` | Markdown | Human-readable report — sorted top-N tables, outliers, action-correlation summaries |
| `raw.json` | Pretty JSON | The aggregate `PerfSnapshot` at exit; useful for diffing two sessions |

`eprintln!` (not `tracing`) is used for the on-exit confirmation because the subscriber may already be tearing down — direct stderr is more reliable.

### Frontend overlay

`<PerfOverlay>` mounts only after `isProfilingEnabled()` resolves true at app mount. When mounted:

- Polls `getPerfSnapshot()` periodically and renders sortable per-span aggregates (count, mean, p50, p95, min, max).
- Listens to `cmd+shift+P` to toggle visibility.
- Has an "Export snapshot" button that calls `exportPerfSnapshot` (writes a one-off `<app_data_dir>/exports/perf-<unix_ts>.json` for ad-hoc capture without app-exit).
- Has a "Reset" button that calls `reset_perf_stats` for clean-slate measurement of a specific scenario.

When `--profiling` is absent, `isProfilingEnabled()` resolves false → `<PerfOverlay>` returns `null` → no DOM, no polling, no shortcut handler effect.

### `perfInvoke` IPC wrapper

```ts
async function perfInvoke<T>(cmd: string, args?: unknown): Promise<T> {
  if (!profiling) return invoke<T>(cmd, args);
  const start = performance.now();
  try {
    return await invoke<T>(cmd, args);
  } finally {
    recordAction(`ipc.${cmd}`, { duration_ms: performance.now() - start });
  }
}
```

Used at every IPC call site that wants attribution. The action lands in the timeline alongside the backend's `ipc.{cmd}` span (added via `#[tracing::instrument(name = "ipc.semantic_search", ...)]`), so the report can show frontend-observed RTT vs backend execution and surface the IPC-overhead delta.

### Action breadcrumbs

The frontend's `recordAction` is called at the user-visible action sites:

```ts
recordAction("settings_open", { via: "shortcut" });
recordAction("modal_close", { image_id: 42 });
recordAction("similar_clicked", { image_id: 42, score: 0.87 });
```

Each call hits `record_user_action` on the backend, which appends a `RawEvent` to the timeline. The on-exit report uses these to correlate the next ≤500 ms of span activity to the action — the user can see "settings_open caused 12 ms of React rendering across 3 components."

### Domain diagnostics (separate from span timing)

Spans answer "how long did it take?". A second category of profiling output answers "what was the system actually doing inside that span?" — embedding L2 norms, tokenizer outputs, score distributions, encoder run summaries, cross-encoder rankings. These are **diagnostics**, not spans.

The `record_diagnostic(name: &str, payload: serde_json::Value)` API in `perf.rs` writes a `RawEvent::Diagnostic { ts_ms, diagnostic, payload }` to the same in-memory log as spans and user-action breadcrumbs. The on-exit `perf_report.rs` renders them in a dedicated `## Diagnostics` section grouped by name. When `--profiling` is absent, `record_diagnostic` is a no-op.

The full diagnostic catalogue (current as of 2026-04-26):

| Diagnostic | Emitted from | Payload contents | Answers |
|------------|--------------|-----------------|---------|
| `startup_state` | `lib.rs::run` setup | DB path, models_dir, model files present, per-encoder embedding counts, total/thumb/orphan counts | "What state did the app boot into?" |
| `cosine_math_sanity` | `lib.rs::run` setup | Synthetic-vector tests (orthogonal, parallel, opposite, zero, dim-mismatch, high-dim-random) — each `{got, expected, passes}` | "Is the cosine math itself correct, or is bad search a math bug?" |
| `cosine_cache_populated` | `cosine/index.rs::populate_from_db_for_encoder` | encoder_id, count, duration_ms | "Which encoder's cache was loaded with how many embeddings?" |
| `embedding_stats` | Same | encoder_id + L2-norm summary (mean/min/max), per-dim mean/std, NaN/Inf counts, first 3 sample embeddings' first 8 dims | "Are embeddings normalised, broken, degenerate?" |
| `pairwise_distance_distribution` | Same | encoder_id + 50-sample × 1225-pair cosine distribution across 11 buckets [-, 0.0)→[0.9, 1.0] + interpretation | "Is the encoder discriminating between images?" |
| `self_similarity_check` | Same | encoder_id + cosine(emb_0, emb_0), expected 1.0 | "Is the cosine math + the embedding well-formed for this cache?" |
| `encoder_run_summary` | `indexing.rs::run_clip_encoder` + `run_trait_encoder` | encoder_id + attempted/succeeded/failed + elapsed_ms + mean_per_image_ms + sample failure paths | "How did the encoder pass go?" |
| `preprocessing_sample` | Same (first batch only) | encoder_id + first_image_path + dim, L2 norm, range, mean, NaN/Inf counts, first 8 dims, interpretation | "Does the first encoded embedding look healthy?" |
| `tokenizer_output` | `commands::semantic::semantic_search` | encoder_id + raw_query + token_count + attention_mask_sum + token_ids + decoded_tokens + interpretation | "How was the user's query chunked into tokens?" |
| `search_query` | `commands::similarity` (both modes) + `commands::semantic` | type, encoder_id, query (image_id or text), cosine_cache_size, full raw cosine list (paths + scores), score_distribution stats, query_embedding stats (semantic only), path_resolution_outcomes (resolved/missed/thumbnail-miss counts + sample missed paths) | "What did cosine actually return for this query, and where did the path-resolution pipeline drop results?" |
| `cross_encoder_comparison` | `commands::similarity` (once per session via AtomicBool) | active_encoder + per-other-encoder top-5 with paths + scores + cache_size + elapsed_ms | "Would another encoder have ranked these images differently?" |

Diagnostics emit at the call site, not via tracing — they are richer than fields-on-a-span and fire only when interesting (per-search, per-cache-load, once-per-session). The `score_distribution`, `query_embedding`, and `path_resolution_outcomes` payloads are nested INSIDE the `search_query` diagnostic rather than emitted separately, because they're meaningful only in the context of a specific query.

The four `cosine/diagnostics.rs` helpers are stateless functions (no `self`) so they can be called from anywhere without coupling to CosineIndex internals:
- `embedding_stats(&[(PathBuf, Array1<f32>)]) -> Value`
- `pairwise_distance_distribution(&[(PathBuf, Array1<f32>)]) -> Value`
- `self_similarity_check(&[(PathBuf, Array1<f32>)]) -> Value`
- `score_distribution_stats(&[f32]) -> Value`

The `interpretation` field convention (a human-readable verdict like "OK", "WARNING — near-zero norm", "BROKEN — NaN in embedding") appears in 5+ diagnostics and is the primary signal for someone reading the report quickly. The detailed numbers are for follow-up; the interpretation tells you whether to bother.

### Tracing instrumentation coverage

The system relies on every relevant code path being instrumented. Current coverage (commit `6e27245`):

| Module | Spans |
|--------|-------|
| `commands/*` | One `ipc.{command_name}` span per command via `#[tracing::instrument]` |
| `indexing.rs` | `pipeline.run` + `pipeline.scan_phase` + `pipeline.thumbnail_phase` + `pipeline.encode_phase` + `pipeline.cosine_repopulate` |
| `model_download.rs` | `model_download.all` + `model_download.head` + `model_download.file` |
| `watcher.rs` | `watcher.start` + `watcher.event` (manual span via `info_span!().entered()` because closures can't carry `#[instrument]`) |
| `cosine/index.rs` | `cosine.populate_from_db` + `cosine.get_similar_images` + `cosine.get_similar_images_sorted` + `cosine.get_tiered_similar_images` |
| `db/*` | TODO: not yet instrumented per-method (DB calls are typically fast enough that aggregate IPC spans suffice; per-method spans would be a follow-up) |
| Frontend | `<Profiler id="masonry">` around the Masonry tree via `onRenderProfiler` callback |

All instruments are info-level so they only fire under `--profiling` (the env filter is `warn,image_browser_lib=info,image_browser=info`).

## Key Interfaces / Data Flow

### Profiling-enabled launch

```
$ cargo tauri dev -- --profiling   # or: PROFILING=1 cargo tauri dev   # or: npm run tauri -- dev --release -- --profiling
   │
main.rs: parse --profiling
       │ set PROFILING_ENABLED to true
       │ build PerfLayer + register with subscriber
       │ paths::exports_dir() → <app_data_dir>/exports/perf-{ts}/
       │ perf::init_session(...) creates the dir, opens timeline.jsonl handle
       │ perf::spawn_flush_thread() starts a 5s loop that drains RawEvent log → file
       ▼
lib.rs::run launches Tauri
   │
   ▼
[normal app interaction]
   │   commands fire spans → PerfLayer aggregates + appends to RawEvent log
   │   user actions fire recordAction → record_user_action IPC → log
   │   flush thread writes log → timeline.jsonl every 5s
   │
   ▼
User closes window → tauri::RunEvent::Exit fires
   │
   ▼
perf_report::render_session_report(session_dir):
   │ flush_to_file(timeline_path) one final time
   │ read_timeline → Vec<RawEvent>
   │ snapshot() → PerfSnapshot (in-memory aggregate)
   │ build_markdown(events, snapshot) → report.md
   │ to_string_pretty(snapshot) → raw.json
   │ eprintln!("profiling report written to ...")
   ▼
process exits
```

### `PerfSnapshot` shape

```json
{
  "spans": [
    { "name": "ipc.semantic_search",
      "count": 14,
      "mean_ns": 245_000_000,
      "min_ns": 89_000_000,
      "max_ns": 612_000_000,
      "p50_ns": 230_000_000,
      "p95_ns": 580_000_000 },
    ...
  ],
  "captured_at_unix": 1745700000
}
```

The percentiles are computed on demand from the recent-samples ringbuffer in `snapshot()`, so they reflect the last ≤200 samples per span.

## Implemented Outputs / Artifacts

- `--profiling` CLI flag handled in `main.rs`
- `perf.rs` (PerfLayer + OnceLocks + flush thread + RawEvent log)
- `perf_report.rs` (on-exit markdown renderer + raw.json + correlation logic)
- `commands/profiling.rs` (5 IPC commands)
- `src/components/PerfOverlay.tsx` (frontend overlay UI)
- `src/services/perf.ts` (wrappers + perfInvoke + onRenderProfiler)
- `<Profiler id="masonry">` integration in `pages/[...slug].tsx` (commit `ee8c5d6`)
- Per-session directory `<app_data_dir>/exports/perf-{unix_ts}/` containing `timeline.jsonl`, `report.md`, `raw.json`
- The historical `context/plans/perf-diagnostics.md` master spec was deleted in upkeep after Phases 1-5 shipped; future-work items (Phase 6 timeline rotation, Phase 7+ system sampling — partially shipped, etc.) are tracked in `notes.md` § Active work areas.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Per-span memory grows linearly with distinct span names | Many short-lived spans with unique names (e.g., one per file path in a hypothetical future instrument) | The HashMap size grows; the recent-samples cap is per-span so many spans × 1.6 KB. Currently bounded by ~50 known span names. |
| Tracing dispatch overhead is non-zero even without `--profiling` | Hot loops with span instrumentation | Few hundred ns per call. Becomes measurable in absolute hot paths (cosine inner loop) but those don't have `#[instrument]` for this reason. |
| 5s JSONL flush could lose the last 5s of events on crash | Process killed (not Exit) before flush | timeline.jsonl is missing the tail. Snapshot in raw.json is always current via in-memory aggregate. Acceptable trade-off — 5s vs per-event flush which would dominate IO. |
| Long sessions create big `timeline.jsonl` | A multi-hour profiling session | File grows ~1 MB per few thousand events. No automatic rotation. The renderer can handle it; the user can manually delete old session dirs. |
| `perf::init_session` failure is non-fatal | Disk full, permissions error in `<app_data_dir>/exports/` | Logs warn; aggregates still work in memory; on-exit renderer returns Err but doesn't crash. The user gets an in-app PerfOverlay with snapshot data but no on-disk artefacts. |
| User-action correlation is heuristic, not causal | Action fires; another action fires within 500 ms; spans correlate to whichever action was first | False attributions for rapid-fire interactions. Not a correctness issue — it's documentation. |
| Exit hook only fires for window-close, not for `cargo tauri dev` SIGTERM | `Ctrl+C` in the dev terminal | The on-exit renderer doesn't run; user must close the window cleanly. Snapshot is still written on demand via `exportPerfSnapshot` button. |
| Profiling-enabled AND code panic could leave perfreport partial | Panic during `render_session_report` | Stderr message reports the failure; partial files may exist. Profile data was still in `timeline.jsonl` for manual analysis. |

## Partial / In Progress

- Per-DB-method instrumentation. Currently only the IPC layer has spans; the per-`db.*` method timings are not separately attributed. Likely to be added if a future audit shows DB calls are dominating an IPC.
- Bundle-size report for the WebView. The frontend's React Profiler timings are captured but bundle-size and resource-load metrics aren't yet correlated.

## Planned / Missing / Likely Changes

- **Session diff tooling.** Two `raw.json` snapshots could be diffed programmatically to surface "this regressed by 30 ms p95 between commits A and B." Today this requires manual comparison.
- **Per-DB method instrumentation** if a future audit shows DB time is dominating an IPC.
- **Memory-pressure histograms.** Currently no RSS / heap tracking. The original master plan calls for this; it's deferred until the user observes a memory issue worth profiling.
- **OTLP export.** The `enhancements/recommendations/10-tracing-otlp-export.md` document discusses exporting via OpenTelemetry to an external collector for long-term archival. Not on the active roadmap.
- **HdrHistogram instead of ringbuffer percentile.** Mentioned in the source comments as a future option if 5+ significant digits at 60-second percentile windows are ever needed. Today the ringbuffer-based approach is sufficient.

## Durable Notes / Discarded Approaches

- **`OnceLock` over `lazy_static!` / `RwLock`.** Single-write-at-init is exactly the OnceLock idiom; no external macros needed.
- **One log file per session, not one append-only file per app install.** Session-scoped directories make it easy to delete old sessions, share a specific run, and avoid contention between sessions if the user runs profiling twice without restarting between runs.
- **The 500 ms correlation window is arbitrary but defensible.** Anything slower than that and the user has already stopped attributing the lag to their last click. Tightening it (e.g., 200 ms) would miss legitimate correlations on slower machines; loosening it (e.g., 2s) would create false attributions for unrelated background activity.
- **Why not `tokio-console`?** Project is sync (not async). `tokio-console` is for Tokio runtimes. The tracing-subscriber Layer pattern fits this codebase better.
- **Why not `flamegraph`?** Excellent for one-off profiling outside the app; this system is designed to be always-available inside the app for arbitrary user scenarios. The two complement each other — `flamegraph` for stack-resolved hotspots, this system for per-operation aggregates and user-action correlation.
- **Flush via background thread, not via async future.** The codebase is sync-heavy; spawning a thread is simpler than dragging in a runtime just for this.
- **`record_user_action` is fire-and-forget on the frontend** — it doesn't await the response. The IPC overhead is ~1 ms; awaiting it would block the user-action event handler unnecessarily.
- **`perfInvoke` is opt-in per call site, not an automatic interceptor.** A global interceptor (e.g., monkey-patching `invoke`) was considered and rejected: the explicit wrapper makes it visible at every call site that profiling is involved, and a future site author can deliberately use plain `invoke` if they want to exclude a specific call from profiling.

## Obsolete / No Longer Relevant

The previous "println!-only" observability layer is dormant. Production code now uses `tracing::info!` / `debug!` / `warn!` everywhere; the `[Backend]` prefix convention is gone. The convention notes have been updated.
