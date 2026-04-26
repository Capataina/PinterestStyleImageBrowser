# Profiling diagnostics expansion

## Header

- **Status:** proposed expansion backlog
- **Date:** 2026-04-26
- **Related plan:** `context/plans/perf-diagnostics.md`
- **Related diagnosis:** `context/plans/performance-analysis.md`
- **Trigger:** the current profiling report identified severe lag but could not conclusively explain whether the 22s `get_images` stalls were caused by DB lock wait, SQL work, IPC serialisation, frontend rendering, system resource starvation, or a combination.

## Executive Summary

The current profiling system is already useful. It records tracing spans, frontend IPC round trips, React Profiler events, user-action breadcrumbs, structured domain diagnostics, a JSONL timeline, `raw.json`, and a human-readable `report.md`.

The missing capability is **causality**.

Today the report can say:

```text
ipc.get_images took 22.31s
siglip2.encode_image was expensive
Masonry renders were slow
```

It cannot yet say:

```text
ipc.get_images waited 21.8s for the foreground DB mutex while SigLIP-2
was saturating CPU, after useIndexingProgress invalidated ["images"],
then Masonry rendered 1989 items and blocked the main thread for 404ms.
```

This plan adds the missing layers: trace hierarchy, span fields, DB/IPC subspans, frontend invalidation tracing, browser-main-thread metrics, resource sampling, automatic stall analysis, report diffing, and reproducible scenario tooling.

## Current Baseline

Implemented today:

| Capability | Current state |
|---|---|
| Backend span timing | `PerfLayer` records aggregate stats and raw close events for tracing spans. |
| Frontend IPC timing | `perfInvoke` records frontend-observed command round trip duration. |
| React render timing | `React.Profiler` wraps Masonry. |
| User breadcrumbs | `recordAction` appends user actions to the timeline. |
| Domain diagnostics | `record_diagnostic` records structured payloads such as startup state, embedding stats, search query score distributions, and encoder summaries. |
| On-exit report | `perf_report.rs` renders top spans, hotspots, outliers, action timeline, per-span table, and diagnostics. |
| Raw artefacts | Each profiling session writes `timeline.jsonl`, `raw.json`, and `report.md`. |

Known current gaps:

| Gap | Why it matters |
|---|---|
| Span events only have close timestamp + duration | Overlap and concurrency are hard to reconstruct. |
| No `span_id` / `parent_id` | Subspans cannot be attached to their command or parent phase. |
| Span fields are not captured | Aggregates collapse all calls by name, losing `encoder_id`, `batch_size`, `row_count`, etc. |
| No DB wait/execute split | A slow IPC cannot be decomposed into lock wait, SQL, aggregation, mapping, or serialisation. |
| No system sampling | Resource starvation is inferred rather than proven. |
| Browser main-thread data is thin | React commit time is captured, but long tasks, FPS, input delay, and layout pressure are not. |
| Report analysis is manual | The report surfaces data, but does not yet explain stalls automatically. |

## Design Principles

| Principle | Rule |
|---|---|
| Causality over count | Prefer data that explains why a stall happened over more top-level span totals. |
| Low-cardinality metadata only | Capture fields such as `encoder_id`, `phase`, `row_count`; avoid full paths, raw query strings, and arbitrary IDs in aggregate keys. |
| Profiling must stay opt-in | `--profile` remains the activation boundary. Normal app runs should not pay for diagnostics. |
| Measure wait separately from work | Lock waits, queue waits, IPC waits, SQL execution, and UI rendering are different bottlenecks. |
| Keep raw data machine-readable | Every human report section should be derivable from JSONL/JSON artefacts. |
| Avoid self-distortion | High-frequency probes need sampling, caps, or thresholds so the profiler does not become the bottleneck. |
| Make reports comparable | Include run metadata so two reports can be diffed without guesswork. |

## Additions Catalogue

### 1. Causal Trace Model

| Addition | Purpose | Implementation notes |
|---|---|---|
| Span `start_ms` and `end_ms` | Reconstruct real overlap instead of approximating from close time. | Store start in span extensions; write both values on close. |
| `span_id` | Uniquely identify a span instance in the timeline. | Generate a monotonic integer in `PerfLayer::on_new_span`. |
| `parent_id` | Link subspans to their parent command/phase. | Read parent span from tracing context when available. |
| Thread id | Identify which OS thread did the work. | Capture `std::thread::current().id()` as a debug string. |
| Thread name | Distinguish frontend IPC handler, encoder worker, thumbnail worker, flush thread. | Use `std::thread::current().name().unwrap_or("unnamed")`. |
| Span lifecycle event shape | Preserve existing aggregates while adding richer raw events. | Add a new raw trace event or extend `RawEvent::Span` compatibly. |
| Active-span snapshot | Show currently running spans in overlay. | Maintain a live map keyed by `span_id`; remove on close. |
| Chrome Trace / Perfetto export | Visualise concurrency with mature tooling. | Export JSON compatible with `chrome://tracing` / Perfetto. |
| Critical-path reconstruction | Identify the sequence that blocked a user-visible action. | Use parent/child IDs plus frontend action markers. |

Recommended raw shape:

```json
{
  "kind": "trace_span",
  "span_id": 1842,
  "parent_id": 1810,
  "ts_start_ms": 466402,
  "ts_end_ms": 488556,
  "duration_us": 22153791,
  "name": "ipc.get_images",
  "thread": "ipc-worker-3",
  "fields": {
    "tag_count": 0,
    "sort_mode": "default"
  }
}
```

### 2. Span Fields and Labels

| Addition | Purpose | Implementation notes |
|---|---|---|
| Field visitor for tracing span attributes | Capture structured fields already passed to `#[instrument]`. | Implement a `Visit` collector in `perf.rs`. |
| Field allowlist | Prevent high-cardinality aggregate explosion. | Keep raw fields but aggregate only approved keys. |
| `encoder_id` field | Split CLIP/SigLIP/DINO activity without relying only on span names. | Add to encoder spans and diagnostics. |
| `batch_size` field | Explain encoder throughput changes. | Add to batch encode spans. |
| `row_count` / `rows_returned` | Connect DB time to result volume. | Add to DB spans and IPC response diagnostics. |
| `image_count` / `total_images` | Connect pipeline phases to workload size. | Add to scan, thumbnail, encode phase spans. |
| `phase` | Identify whether work happened during startup, scan, thumbnail, encode, ready, idle. | Add a profiling phase state or emit phase transition diagnostics. |
| `query_key` summary | Attribute frontend query work without dumping raw keys. | Hash or compactly label keys such as `images`, `pipelineStats`, `semanticSearch`. |
| `trigger` / `source` | Explain why a fetch happened. | Example: `indexing_progress_encode`, `tag_mutation`, `manual_refresh`. |

Field capture needs a policy distinction:

| Field class | Example | Store raw event | Aggregate key |
|---|---|---:|---:|
| Low-cardinality | `encoder_id=siglip2_base` | yes | yes |
| Medium-cardinality | `root_id=3` | yes | maybe |
| High-cardinality | full file path | only in diagnostics when needed | no |
| Sensitive | raw search text | avoid by default or redact | no |

### 3. Backend DB and IPC Decomposition

| Addition | Purpose | Implementation notes |
|---|---|---|
| `db.connection_lock_wait` | Prove/disprove mutex contention. | Time before acquiring the foreground DB connection lock. |
| `db.get_images.sql_prepare` | Find statement preparation cost. | Wrap `conn.prepare`. |
| `db.get_images.sql_execute` | Find SQLite execution latency. | Wrap query execution call. |
| `db.get_images.row_iteration` | Measure row fetching loop. | Time the `query_map` / row iteration section. |
| `db.get_images.aggregate_rows` | Measure Rust-side grouping. | Wrap tag/image aggregation. |
| `db.get_images.map_to_api` | Measure conversion to `ImageData`. | Wrap final mapping. |
| `db.get_images.sort` | Measure final ordering. | Wrap sort only. |
| `db.get_images.explain_plan` diagnostic | Detect bad query plans. | Emit on slow query or once per session. |
| `db.busy_or_locked_count` diagnostic | Detect SQLite contention. | Increment on busy/locked errors or retry loops. |
| WAL size sample | Detect checkpoint pressure. | Sample `images.db-wal` bytes. |
| DB file size sample | Track growth and compare runs. | Include in startup/system samples. |
| Response row count | Connect IPC cost to payload size. | Record image rows and joined tag rows. |
| Response byte estimate | Identify serialisation pressure. | Approximate via serialised JSON length only in profiling mode. |
| Backend vs frontend IPC delta | Separate command body from Tauri boundary overhead. | Report `frontend_rtt - backend_span_duration`. |
| In-flight IPC count | Detect piling calls. | Increment/decrement around `perfInvoke` or backend command start/end. |

First target:

```text
ipc.get_images
  db.connection_lock_wait
  db.get_images.sql_prepare
  db.get_images.sql_execute
  db.get_images.row_iteration
  db.get_images.aggregate_rows
  db.get_images.map_to_api
  db.get_images.sort
  ipc.get_images.response_size
```

### 4. Encoder and ML Diagnostics

| Addition | Purpose | Implementation notes |
|---|---|---|
| `encoder.preprocess` subspan | Split image decode/resize/normalise from model inference. | Add per-encoder consistent names. |
| `encoder.inference` subspan | Measure pure ONNX/ORT runtime. | Wrap session run only. |
| `encoder.db_write` subspan | Measure embedding persistence. | Wrap batch insert/update. |
| `encoder.queue_depth` diagnostic | Show pending work. | Count missing embeddings for each encoder. |
| `encoder.active_state` event | Show which encoder is currently running. | Emit phase transition or active worker diagnostic. |
| `encoder.throughput_window` diagnostic | Detect mid-run slowdown. | Emit images/sec over rolling 30s windows. |
| `encoder.memory_delta` diagnostic | Attribute model RSS impact. | Sample RSS before/after model load where possible. |
| ORT provider diagnostic | Confirm CPU/CoreML/Metal provider reality. | Record configured and effective execution providers. |
| Model load time per encoder | Separate load stalls from encode stalls. | Instrument model session creation. |
| Per-image skip/failure counts | Detect repeated wasted work. | Emit counts and sample failure reasons. |
| Thermal-throttling proxy | Detect sustained throughput collapse. | Report rolling throughput degradation, even without OS thermal APIs. |

Useful report example:

```text
SigLIP-2 throughput degraded from 5.8 img/s to 0.9 img/s between t=450s
and t=545s while get_images stalls occurred.
```

### 5. Frontend Query and Data-Flow Diagnostics

| Addition | Purpose | Implementation notes |
|---|---|---|
| React Query invalidation event | Show who invalidated a query. | Wrap `queryClient.invalidateQueries` behind a helper. |
| Query key summary | Identify `["images"]` vs other queries. | Compact string form with redaction. |
| Invalidation source | Attribute refreshes to indexing progress, tag mutation, folder mutation, ready event. | Pass source explicitly. |
| Query fetch start/end | Measure frontend-visible query lifecycle. | Wrap query functions or use QueryCache subscription. |
| Cache hit/stale state | Detect unnecessary refetches. | Subscribe to TanStack Query state changes. |
| Response item count | Connect render cost to list size. | Record `images.length`. |
| Response mapping time | Measure backend row to `ImageItem` conversion. | Wrap `map`. |
| `convertFileSrc` count/time | Detect expensive URL conversion. | Time the path conversion loop. |
| Sort/filter derivation time | Measure frontend data transforms. | Wrap sort/filter work in `useImages` consumers. |
| Display set change size | Detect full churn vs small update. | Compare previous/current image IDs. |
| Query waterfall report | Show sequences such as invalidate -> fetch -> map -> render. | Report section built from frontend events. |

Target causal chain:

```text
indexing_progress_tick(phase=encode)
  -> query_invalidated(key=images, source=indexing_progress_encode)
  -> frontend.fetch_images(duration=28832ms)
  -> frontend.map_images(count=1989)
  -> react.masonry.render(duration=404ms)
```

### 6. Browser Main-Thread and UI Smoothness

| Addition | Purpose | Implementation notes |
|---|---|---|
| `PerformanceObserver` long tasks | Detect main-thread blocks >50ms. | Browser support varies; emit capability diagnostic. |
| FPS estimate | Show actual smoothness. | Use `requestAnimationFrame` sampling while profiling. |
| Dropped-frame count | Quantify user-visible jank. | Count frame deltas >32ms / >50ms / >100ms. |
| Input delay | Measure lag between event timestamp and handler execution. | Use event timestamps for click/keydown/scroll. |
| Scroll jank sampling | Identify Masonry scroll pressure. | Record scroll event delays and render bursts. |
| Layout/reflow timing where available | Detect layout thrash. | Use browser performance entries when supported. |
| Paint timing | Track first paint and major visible updates. | Useful for startup diagnostics. |
| Component render reason | Explain why a component rerendered. | Lightweight prop-change summaries in profiling mode. |
| Masonry visible item count | Decide whether virtualisation is required. | Record visible count vs total count. |
| Masonry item render count | Detect full-grid rerenders. | Profiler around item or instrumentation in map. |
| Animation pressure | Detect many simultaneous Framer Motion animations. | Count mounted motion elements or animation starts. |

React Profiler tells us commit duration. Browser main-thread diagnostics tell us whether the app felt frozen.

### 7. System Resource Sampling

| Addition | Purpose | Implementation notes |
|---|---|---|
| Process RSS | Detect memory pressure and model footprint. | Sample every 1s under `--profile`. |
| JS heap size | Detect frontend memory growth. | Browser support varies; emit capability diagnostic. |
| CPU usage percent | Detect saturation. | Platform-specific implementation. |
| Thread count | Detect worker leaks or thread explosion. | Platform-specific or process introspection. |
| Open file descriptor count | Detect file-handle leaks. | Unix-first, platform-gated. |
| Disk read/write bytes | Identify I/O bottlenecks. | Platform-specific; optional. |
| DB/WAL file sizes | Detect SQLite checkpoint pressure. | Cheap path metadata sample. |
| App data directory sizes | Track thumbnails/models/cache footprint. | Sample at startup and on exit. |
| System load average | Detect external process interference. | Unix/macOS where available. |
| Battery / low-power mode | Explain macOS throttling. | Optional macOS-specific diagnostic. |
| Thermal pressure proxy | Explain sustained slowdown. | Use throughput collapse if OS API unavailable. |

Recommended sample event:

```json
{
  "kind": "system_sample",
  "ts_ms": 545000,
  "rss_mb": 2134,
  "cpu_percent": 380.0,
  "threads": 42,
  "db_wal_mb": 18.2,
  "open_fds": 131
}
```

### 8. Report Intelligence

| Addition | Purpose | Implementation notes |
|---|---|---|
| Automatic stall detector | Find spans/actions above thresholds. | Example: backend span >1s, render >100ms, long task >50ms. |
| Stall neighbourhood section | Summarise ±30s around each stall. | Include active spans, system samples, user actions, diagnostics. |
| Outlier clustering | Group repeated similar stalls. | Avoid 20 duplicate rows. |
| Likely-cause heuristics | Turn raw data into hypotheses. | Rules must cite evidence, not pretend certainty. |
| Phase-separated summaries | Separate startup, scan, thumbnail, encode, idle. | Use phase transition diagnostics. |
| Foreground/background split | Show user-facing cost vs background indexing. | Based on action correlation and phase. |
| Backend/frontend delta table | Show IPC boundary cost. | Pair `ipc.foo` backend spans with frontend `ipc_call`. |
| Query invalidation summary | Show which sources caused refetches. | Group by key and source. |
| Render pressure section | Show React/long-task/FPS stats together. | User-visible smoothness section. |
| Resource-pressure section | Show CPU/RSS/thread/WAL trends. | Detect starvation and memory growth. |
| Threshold alert section | Surface violations against expected budgets. | Keep budgets in one config/table. |
| Report recommendations | Suggest next action from rules. | Example: “virtualise Masonry” when render p95 >50ms at >1000 items. |

Example stall section target:

```text
## Stall cluster: get_images during encode

2 events, max 22.31s, both during phase=encode.

Likely causes:
- db.connection_lock_wait dominated 98% of ipc.get_images duration.
- SigLIP-2 throughput dropped 6.4x in the same 95s window.
- ["images"] was invalidated by indexing_progress_encode before both calls.
- Masonry rendered 1989 items after the second response and took 404ms.

Confidence: high for DB lock wait; medium for CPU starvation.
```

### 9. Report Metadata and Comparability

| Addition | Purpose | Implementation notes |
|---|---|---|
| Git SHA | Identify exact code. | Include dirty flag. |
| Build profile | Distinguish dev/release. | Include Tauri dev/release mode. |
| App version | Compare packaged builds. | From Cargo/package metadata. |
| OS and architecture | Compare machines. | Include macOS version, CPU arch. |
| Hardware summary | Explain performance envelopes. | CPU model/core count, RAM where available. |
| Selected encoder settings | Explain search/indexing behaviour. | Include active image/text encoder choices. |
| Library counts | Images, thumbnails, embeddings by encoder, roots. | Already partly in `startup_state`; promote to header. |
| Model file state | Present/missing/file sizes. | Already partly present; include versions/checksums if feasible. |
| Scenario markers | Name the run: folder add, scroll, search, idle. | Manual marker button or command. |
| Report schema version | Keep tooling compatible. | Include in `raw.json` and timeline. |

### 10. Tooling and Workflow

| Addition | Purpose | Implementation notes |
|---|---|---|
| `perfdump` CLI | Query reports without ad-hoc `jq`. | Read `timeline.jsonl` and print selected summaries. |
| `perfdiff` CLI | Compare two sessions. | Diff p95/max/count/resource trends. |
| Scenario runner | Reproduce known workloads. | Example: add 2k images, scroll grid, semantic search. |
| Synthetic image corpus | Stable benchmark input. | Avoid dependence on user library. |
| Perf regression tests | Catch obvious slowdowns. | Bench pure hot paths with deterministic data. |
| Criterion benches | Statistical microbenchmarks. | Use for cosine, query mapping, top-k, serialisation. |
| Report retention/rotation | Prevent unbounded growth. | Rotate timeline and prune old sessions. |
| Export zip | Share one profiling bundle. | Include report, raw, timeline, metadata, maybe redacted config. |
| Redaction mode | Make reports shareable. | Strip paths/search text when requested. |
| OTLP export | External observability. | Optional config, disabled by default. |
| Perfetto export | Local visual trace. | Low dependency, high diagnostic value. |

## Implementation Phases

### Phase 1: Causal trace substrate

Goal: make the raw timeline capable of representing overlap and parent/child causality.

- [ ] Add `span_id`, `parent_id`, `start_ms`, `end_ms`, `thread_id`, and `thread_name` to raw span events.
- [ ] Preserve existing aggregate `PerfSnapshot` behaviour.
- [ ] Capture low-cardinality span fields with a tracing field visitor.
- [ ] Add a trace-schema version to timeline events.
- [ ] Add tests for raw event serialisation and backwards-compatible report parsing.
- [ ] Add a Perfetto/Chrome trace export path.

### Phase 2: `get_images` and DB decomposition

Goal: make the next `get_images` stall explainable without manual inference.

- [ ] Add lock-wait timing around foreground DB connection access.
- [ ] Add `get_images_with_thumbnails` subspans for SQL prepare, SQL execution, row iteration, aggregation, API mapping, and sorting.
- [ ] Add row-count and response-size diagnostics for `get_images`.
- [ ] Emit `EXPLAIN QUERY PLAN` diagnostic for `get_images` once per session or on slow query.
- [ ] Add a report section pairing frontend `ipc.get_images` RTT with backend subspan totals.
- [ ] Re-run the 2k-folder-add scenario and confirm the report identifies the dominant component of each stall.

### Phase 3: Frontend causality and main-thread metrics

Goal: connect invalidations, query work, render work, and user-visible jank.

- [ ] Wrap query invalidations in a profiling-aware helper with `key` and `source`.
- [ ] Add React Query cache/fetch lifecycle events in profiling mode.
- [ ] Time `fetchImages` mapping, `convertFileSrc`, sort/filter derivation, and display set churn.
- [ ] Add `PerformanceObserver` long-task collection where available.
- [ ] Add `requestAnimationFrame` FPS and dropped-frame sampling while profiling.
- [ ] Add input delay sampling for click, keydown, and scroll events.
- [ ] Expand the report with a frontend data-flow section.

### Phase 4: System resource sampler

Goal: prove or disprove resource starvation.

- [ ] Add 1Hz process RSS sampling.
- [ ] Add 1Hz CPU usage sampling.
- [ ] Add thread count sampling.
- [ ] Add DB/WAL file size sampling.
- [ ] Add optional open-FD sampling on supported platforms.
- [ ] Add capability diagnostics for unsupported platform metrics.
- [ ] Add resource trend charts/tables to `report.md`.

### Phase 5: Automatic stall analysis

Goal: make the report explain the run rather than only list facts.

- [ ] Detect backend stalls, frontend stalls, long tasks, dropped-frame bursts, and render outliers.
- [ ] Build ±30s neighbourhood summaries around each stall cluster.
- [ ] Correlate stalls with active spans, query invalidations, system samples, encoder throughput, and render events.
- [ ] Add evidence-backed likely-cause heuristics with confidence labels.
- [ ] Add threshold alert table.
- [ ] Add phase-separated summaries.

### Phase 6: Diffing and reproducibility

Goal: compare runs and catch regressions.

- [ ] Add git/build/OS/hardware/run metadata to every report.
- [ ] Add `perfdump` CLI.
- [ ] Add `perfdiff` CLI.
- [ ] Add a scenario marker API and UI button.
- [ ] Add a repeatable folder-add / scroll / search scenario runner.
- [ ] Add report retention and timeline rotation.
- [ ] Add export zip with optional redaction.

## Recommended First Slice

The first implementation slice should be deliberately narrow:

```text
1. Add richer raw span events with start/end time and parent ids.
2. Add get_images DB subspans.
3. Add query invalidation source events for ["images"].
4. Add a simple 1Hz RSS/CPU/thread sampler.
5. Add one "Stall analysis" report section for spans >1s.
```

That slice directly addresses the unresolved lag diagnosis and avoids prematurely building every planned observability surface.

## Success Criteria

- [ ] A future `get_images` stall can be decomposed into wait time, SQL time, Rust aggregation/mapping time, IPC overhead, and frontend render cost.
- [ ] A future report can show whether SigLIP/DINO/CLIP work overlapped a user-visible stall.
- [ ] A future report can show whether the UI was janky via long tasks, dropped frames, or input delay, not just React commit duration.
- [ ] A future report can prove or disprove resource starvation with CPU/RSS/thread/WAL evidence.
- [ ] Two profiling sessions can be compared without manual `jq`.
- [ ] The profiling system remains off by default and low-overhead when profiling is disabled.
- [ ] High-cardinality or sensitive data does not leak into aggregate keys.

## Risks and Counter-Scenarios

| Risk | Trigger | Mitigation |
|---|---|---|
| Profiler overhead distorts the run | Too many high-frequency events, system sampling too frequent, serialising large payloads | Keep sampling coarse, cap events, emit expensive diagnostics only on slow paths. |
| Trace data becomes noisy | Every component emits low-value events | Prioritise causal transitions, waits, and user-facing work; avoid mechanical logging. |
| Aggregate cardinality explodes | Span fields include paths, raw queries, image IDs | Field allowlist and aggregate-key policy. |
| Report becomes unreadable | Every addition gets a full section | Cluster, summarise, and put details behind collapsible blocks or raw artefacts. |
| Browser APIs are unavailable in WebKit/Tauri | Long Task, JS heap, or layout entries unsupported | Emit capability diagnostics and degrade gracefully. |
| System metrics are platform-specific | macOS/Linux/Windows APIs differ | Implement cheap cross-platform subset first; gate platform-specific metrics. |
| Causality is overstated | Correlation window sees unrelated background work | Reports must phrase likely causes with evidence and confidence, not certainty. |
| OTLP/export features distract from fixing lag | Observability story becomes portfolio-driven | Keep OTLP optional and later; fix causal local diagnostics first. |

## Assumptions Needing Stronger Evidence

- **Assumption:** the biggest profiling weakness is causal explanation, not raw coverage. This is supported by the latest lag report, but another workload could reveal an uninstrumented subsystem instead.
- **Assumption:** local JSONL/Markdown/Perfetto artefacts are still the right centre of gravity. If the project becomes multi-machine or long-running, OTLP export may become more important.
- **Assumption:** 1Hz system sampling is enough to explain resource pressure. If stalls happen in sub-second bursts, finer sampling or event-triggered snapshots may be needed.
- **Assumption:** frontend invalidation tracing will explain a large part of UI jank. If Masonry remains slow even with no refetches, virtualisation and render architecture become the primary fix.

## Not First

These are valuable but should not precede the causal local diagnostics:

| Deferred item | Reason |
|---|---|
| OTLP export | Good portfolio/production-observability signal, but does not directly explain the current local lag. |
| Full flamegraph integration | Useful for CPU stack hotspots, but the current mystery is cross-layer causality. |
| Always-on production telemetry | Violates local-first expectations unless explicitly designed and consented. |
| Per-image trace events for every tiny operation | High event volume and privacy risk; use sampling or slow-path diagnostics instead. |
| SQL index changes driven only by current aggregates | The current data suggests contention/starvation; subspans should prove the bottleneck first. |

