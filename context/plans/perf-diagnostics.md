# Performance diagnostics — master plan

A complete profiling system for the Image Browser. The goal is that
every potentially-slow operation in the app emits structured timing
data, those samples are aggregated into histograms in memory, and
the user can see what's actually slow in a live overlay without
having to reach for `cargo flamegraph`.

This is a plan document. Sections marked **(implemented)** describe
what's already shipped; **(planned)** is what's still TODO.

---

## Why this exists

The app has multiple long-running operations and a lot of cross-
boundary cost (Rust ↔ webview IPC, ONNX inference, SQLite I/O,
JPEG codec work, masonry layout). Without aggregation, the only
signal we get is unstructured `tracing::info!` lines in the
terminal. That's enough to tell you that something happened, not
to tell you which N% of operations took longer than they should.

A profiling system that surfaces every layer's latency in one place
turns "the app feels slow" into "the encode batches in 12s, masonry
layout takes 8ms but fires 30 times per scroll, IPC roundtrip is
2.4ms p50."

---

## Layers we care about

| Layer | Examples | Worst-case impact |
|---|---|---|
| Startup | Tauri runtime init, DB migrations, React mount | Visible blank window time |
| Pipeline | Scan, thumbnail, encode, cosine populate | Pill-visible duration |
| ML inference | CLIP image encode batch, text encode | Encoding pass total time |
| IPC | Tauri invoke RTT, JSON serialise size | Click-to-result latency |
| SQL | get_images_with_thumbnails JOIN, get_all_embeddings batch | Grid load latency |
| Image codec | JPEG decode for thumb, encode for thumb | Initial pass throughput |
| Cache I/O | Cosine cache save/load, thumbnail disk reads | Cold start latency |
| React | Masonry layout, MasonryItem render | Scroll smoothness |
| DOM | Layout recompute, paint | Frame drop, jank |
| Memory | Heap, encoder model RSS, peak during encode | OOM risk on smaller machines |
| Disk | Thumbnail write throughput, DB writes per second | Pipeline bottleneck |
| Watcher | Debounced rescan trigger frequency | Background CPU |

---

## Collection mechanisms

### Backend: tracing-based span timing **(planned)**

Every meaningful function gets `#[tracing::instrument]` or a manual
`span!()` wrapping. Spans accumulate in a custom `tracing-subscriber`
Layer that records enter→exit duration as a sample.

Examples to instrument:

- **Tauri commands** — every `#[tauri::command]` wrapped. Span name
  is the command name, fields are arg sizes (Vec<i64> filter_tag_ids
  length, query string length, etc.).
- **Indexing pipeline phases** — one span per phase, with
  `processed`/`total` as fields and elapsed time as the sample.
- **`Encoder::encode_batch`** — span per call, batch_size field.
- **`Encoder::preprocess_image`** — per-image span (could be
  high-frequency, may need sampling).
- **`TextEncoder::encode`** — span per query.
- **`SimpleTokenizer::encode`** — sub-span inside text encode.
- **`ImageScanner::scan_directory`** — recursive, so per-call span.
  Field: number of paths returned.
- **`ThumbnailGenerator::generate_thumbnail`** — per-image span.
  Sub-spans for decode / resize / encode steps if we want
  granularity.
- **CosineIndex retrieval modes** — one span per get_*similar*
  method, each with the cached_images count as a field.
- **`CosineIndex::populate_from_db`** — single span, total embeddings
  loaded as field.
- **`db::*` method group** — instrument every public DB method.
  Mostly fast (microseconds) but the cumulative count matters.
- **`model_download::download_to_file`** — per-file span, total bytes
  downloaded as field.

### Backend: aggregation **(planned)**

`src-tauri/src/perf.rs` registers a `Layer` that:

1. Collects every span's duration as a `u128` nanosecond sample.
2. Stores per-span-name histograms in a `Mutex<HashMap<String,
   Histogram>>`. Histogram tracks: count, sum, min, max, p50, p95,
   p99 (HdrHistogram or similar).
3. Keeps a fixed-size ringbuffer (last N=200 samples per span name)
   for "show me the recent calls" UI views.
4. Periodically flushes the aggregate snapshot to
   `Library/perf.jsonl` (one JSON line per flush, every ~30s).
5. Exposes `get_perf_snapshot()` Tauri command returning the live
   aggregate as serialisable structures.

The HdrHistogram crate is the cleanest dependency for this. The
flush-to-disk format is JSONL so users can inspect post-hoc with
`jq`, and we can compare across versions.

### Frontend: Performance API + React Profiler **(planned)**

- Wrap `invoke()` from `@tauri-apps/api/core` in a `perfInvoke()`
  helper that brackets the call with `performance.mark` /
  `performance.measure`. The mark name is the command name; the
  measure produces a duration that we can query later.
- Wrap key components with React's `<Profiler>`. The render-duration
  callback is fed into a per-component histogram.
- A periodic timer (~10s) collects all `performance.getEntriesByType`
  samples + the React profiler aggregates and ships them to the
  backend via a `record_frontend_perf` Tauri command. Backend appends
  them to the same JSONL file as the backend metrics.

### Browser-level perf **(planned, lower priority)**

The webview supports `PerformanceObserver` for:
- Layout shifts (CLS — but we're not really a content site)
- Long tasks > 50ms (worth flagging, blocks main thread)
- Largest Contentful Paint, First Contentful Paint
- Frame timing API (when available; webkit may not have it)

We collect what's available, log what isn't.

### System-level metrics **(planned)**

Backend periodically samples:
- Process RSS via `procfs` (Linux) / `mach::*` (macOS) / Win32 PSAPI
- File descriptors open count
- Threads spawned (via `std::thread::available_parallelism` × tracking)

These aren't per-operation; they're sampled at a 1Hz cadence into
the same JSONL.

### Network metrics **(planned)**

`model_download` already logs the URL + final size. Add per-chunk
throughput averaged over the chunk window (256 KB). Surface as
`download.throughput_mbps` in the perf overlay.

---

## Surfaces

### Terminal output (existing — `tracing::info!`)

Already in place. Useful during dev but not aggregated. Keep, but
the perf system supplements it.

### `Library/perf.jsonl` **(planned)**

Append-only JSONL of snapshots. Format:

```json
{"timestamp": 1729980000, "kind": "snapshot", "spans": {
  "encode_batch": {"count": 12, "p50_us": 5234.1, "p95_us": 6021.3, ...},
  "get_images": {"count": 47, "p50_us": 3.2, ...}
}}
{"timestamp": 1729980000, "kind": "system", "rss_mb": 624, "threads": 12}
```

Rotated when it exceeds ~10 MB; previous files renamed
`perf-1.jsonl`, `perf-2.jsonl` and pruned beyond 5.

### In-app overlay **(planned)**

Toggle: `cmd+shift+P` (mnemonic: P for Perf). Floating right-side
panel showing:

- **Live spans** — currently-active operations with elapsed time
- **Top 10 slowest** by p95 from the recent histogram
- **Histograms** for each span: bar chart of recent durations
- **System** — RSS, thread count, FPS estimate
- **Frontend renders** — per-component p50/p95
- **Copy snapshot** button → clipboard (paste into a debug session)

Implementation: a new component subscribed to a polling loop that
calls `get_perf_snapshot()` every 2 seconds.

### CLI dump **(planned, optional)**

`cargo run --bin perfdump` reads `Library/perf.jsonl` and prints a
human-readable summary. Useful for offline analysis or CI runs.

---

## Per-layer breakdown — what to track and why

### Startup
- `app_init` span — from `main()` entry to Tauri window open
- `db_init` span — schema + migrations
- `frontend_mount` — main.tsx → App rendered first frame
- Goal: window opens in < 2s on M-series, < 5s on Intel/Linux

### Indexing pipeline
- `pipeline.scan` — total scan time, with per-root sub-spans
- `pipeline.thumbnail` — total + average per-image
- `pipeline.encode` — total + per-batch + per-image-derived
- `pipeline.cosine_populate` — total + per-row average
- Goal: thumbnail pass parallel-scales with cores; encode dominated
  by ONNX (current bottleneck)

### ML inference
- `clip.encode_image` — per-image (preprocess + ONNX run)
- `clip.encode_image.preprocess` — sub-span (decode + resize + norm)
- `clip.encode_image.onnx_run` — pure ONNX time
- `clip.encode_text` — per-query
- `clip.encode_text.tokenize` — sub-span
- `clip.encode_text.onnx_run` — sub-span
- Goal on M-series CPU: ~200-500ms per image batch, ~50-150ms per
  text query. Anything significantly slower means the model loaded
  poorly or there's contention.

### IPC
- `ipc.invoke.<command_name>` — per-call duration on the Rust side
  (Tauri command body)
- `ipc.frontend.<command_name>` — frontend-measured RTT including
  Tauri's serialise/deserialise overhead
- The delta between the two is the IPC overhead itself

### SQL
- `db.<method_name>` for every DB method
- Field: rows touched
- Goal: most DB calls < 1ms; `get_images_with_thumbnails` < 50ms for
  10k images; `get_all_embeddings` < 500ms for 10k embeddings

### Image codec
- `thumbnail.decode` — JPEG decode time
- `thumbnail.resize` — image::thumbnail() call
- `thumbnail.encode` — JPEG encode time
- `thumbnail.write` — disk write
- Goal: per-image total < 200ms

### Cache I/O
- `cosine_cache.save` — bincode + write
- `cosine_cache.load` — read + bincode + push
- Goal: load < 100ms for 10k embeddings (bincode is fast)

### React render
- `<Profiler id="masonry">` — Masonry component render
- `<Profiler id="masonry-item">` — individual tile renders
- `<Profiler id="search-bar">`
- `<Profiler id="status-pill">`
- Goal: all renders < 16ms (60fps budget). Anything > 50ms is a long
  task and visible jank.

### DOM/paint
- Long tasks via PerformanceObserver
- Layout shifts (probably negligible for our app)

### Memory
- Process RSS sampled every 1s
- Goal: stable when not actively encoding; spike during encode then
  return to baseline

### Disk
- Thumbnail bytes written per second (computed)
- DB write count per second (counter)

### Watcher
- `watcher.debounce_window` — events per debounce
- `watcher.rescan_triggered` — count
- Frequency tells us if file changes are bursty vs sustained

---

## Baselines and alerts

Once the system is collecting, baseline runs establish what
"normal" looks like:

| Operation | Expected p95 (M-series CPU) | Alert threshold |
|---|---|---|
| `get_images` | 5ms | 100ms |
| `semantic_search` | 200ms | 1000ms |
| `clip.encode_image` (per image) | 250ms | 600ms |
| `clip.encode_text` | 100ms | 400ms |
| `thumbnail.decode+resize+encode` | 100ms | 400ms |
| `cosine.populate_from_db` (1k images) | 50ms | 200ms |
| `cosine_cache.load` (1k embeddings) | 30ms | 200ms |
| Masonry render | 8ms | 50ms |

The overlay surfaces alerts inline when a recent measurement exceeds
the threshold. Acts as a regression canary.

---

## Implementation phases

| Phase | Scope | Status |
|---|---|---|
| 0 | Plan document | **(this file)** |
| 1 | tracing-instrument all key functions | **(implemented — commits `5910966`, `7e49b75`, `6e27245`; see `systems/profiling.md` § Tracing instrumentation coverage)** |
| 2 | perf::Layer that collects samples + aggregates | **(implemented — `src-tauri/src/perf.rs`)** |
| 3 | get_perf_snapshot Tauri command | **(implemented — `commands/profiling.rs`)** |
| 4 | In-app overlay component, cmd+shift+P toggle | **(implemented — `src/components/PerfOverlay.tsx` + `pages/[...slug].tsx` shortcut handler)** |
| 5 | Frontend perfInvoke + React Profiler integration | **(implemented — `src/services/perf.ts::perfInvoke`, `onRenderProfiler`; commit `ee8c5d6`)** |
| 6 | JSONL flush + rotation | **(implemented for flush — `perf::spawn_flush_thread`, `perf_report::render_session_report`; rotation NOT implemented — see "Still pending" below)** |
| 7 | System-level sampling (RSS, threads) | **(NOT implemented — see "Still pending" below)** |
| 8 | CLI dump tool (`cargo run --bin perfdump`) | **(deferred — out of scope)** |
| 9 | tracing-tracy integration for live viz | **(deferred — optional)** |
| 10 | Cargo benches for pure-fn hot paths | **(deferred — separate concern)** |

Phases 1-5 are fully shipped — the user's `--profiling` mode produces
the on-exit `report.md` + `raw.json` + `timeline.jsonl` artefacts
documented in `systems/profiling.md`. Phase 6 partially landed; Phase
7 hasn't started.

## Still pending

- **Per-DB-method instrumentation** (Phase 1 carry-over) — currently only
  the IPC layer has spans; per-`db.*` method timings aren't separately
  attributed. Adding `db.<method_name>` spans would let the perf report
  show per-DB-method time inside an IPC.
- **Timeline rotation** (Phase 6) — long sessions produce a single
  unbounded `timeline.jsonl`. Rotating at e.g. 100 MB or 1-hour
  boundaries would prevent the file from growing forever in
  always-on profiling sessions.
- **System-level sampling** (Phase 7) — RSS / heap / thread count
  every 1s. Today's metrics are span-based only; resource-pressure
  signals are missing. Not a blocker until a real memory issue surfaces.
- **Session-diff tooling** — two `raw.json` snapshots could be diffed
  programmatically to surface "this regressed by 30 ms p95 between
  commits A and B." Today this requires manual comparison.

These are tracked here rather than as separate plan files because
they're incremental extensions of the shipped system, not a new
feature. When one becomes urgent, lift it into its own plan file or
just implement it inline.

---

## Domain diagnostics (added 2026-04-26)

This plan was originally about **span-timing** profiling. A second
layer landed on top of it: **domain diagnostics** — `record_diagnostic(name, payload)`
calls that emit structured `serde_json::Value` snapshots into the
same RawEvent log as spans. The on-exit `perf_report.rs` renders them
in a dedicated `## Diagnostics` section grouped by name.

12 diagnostics shipped: `startup_state`, `cosine_math_sanity`,
`cosine_cache_populated`, `embedding_stats`,
`pairwise_distance_distribution`, `self_similarity_check`,
`encoder_run_summary`, `preprocessing_sample`, `tokenizer_output`,
`search_query` (with nested `query_embedding`, `score_distribution`,
`path_resolution_outcomes` blocks), `cross_encoder_comparison`.

Spans answer "how long?". Diagnostics answer "what was the system
actually doing?" — embedding L2 norms, tokenizer output, score
distributions, encoder run summaries, cross-encoder rankings.

Full catalogue + emission patterns + `record_diagnostic` API live in
`systems/profiling.md` § Domain diagnostics. The convention is
captured in `notes/conventions.md` § Domain diagnostics via
`record_diagnostic`. The four stateless helpers used by both
`cosine/index.rs` and `commands/*` live in
`src-tauri/src/similarity_and_semantic_search/cosine/diagnostics.rs`.

Status: **shipped** as part of the 2026-04-26 encoder pipeline
overhaul. Treated as a complete capability, not a phase — bumps to
the catalogue land via direct edits to the relevant code path +
`systems/profiling.md`, not via this plan file.

---

## Costs and trade-offs

- **Span overhead**: tracing's `instrument` is cheap but not free.
  ~50-200ns per span. For 200ms operations, irrelevant. For
  microsecond-level DB calls, it's measurable. Decision: instrument
  the DB layer at function granularity but skip per-row sub-spans.
- **Histogram memory**: HdrHistogram with 0..60s range and 3 sig
  digits is ~150 KB per span name. For 50 distinct span names that's
  ~7 MB peak — fine.
- **Frontend Profiler overhead**: in dev mode it's noticeable,
  in production builds it's gated by `NODE_ENV`. We accept the dev
  cost; it's the only way to see render durations.
- **JSONL growth**: a 30s flush cadence with ~50 spans per snapshot
  produces ~5KB/snapshot. ~600 snapshots before rotation = ~3 MB.
  Manageable.
- **The observer effect**: instrumenting heavy operations measures
  them in the presence of the instrumentation. The numbers are
  conservative (slightly higher than reality) but the relative
  ordering is preserved. Don't compare to off-instrumentation
  baselines.

---

## What this DOESN'T cover

- GPU profiling (we're CPU-only since CoreML disable)
- Network profiling beyond model download (we don't make other
  network calls)
- Actual sampling profilers like Instruments/Tracy/perf — those are
  external tools the user runs, not part of this in-app system
- Correctness assertions in performance tests (separate test suite
  concern, covered by cargo test)

---

## Why now

The app's been getting slower as we add features (multi-folder, the
watcher, the persistent cache). Without measurement we're
guessing. The user's specific complaint ("entire app refreshes") is
also a perf-perception issue — once we can see render frequency we
can confirm whether it's a layout cost or just visual jank from the
shuffle. Building this once means every future performance question
gets answered by data, not anecdote.
