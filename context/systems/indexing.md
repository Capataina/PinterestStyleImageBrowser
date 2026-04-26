# indexing

*Maturity: comprehensive*

## Scope / Purpose

The background pipeline that turns a freshly-launched (or freshly-triggered) app into a usable catalogue. Owns the orchestration of: model download (if needed), text-encoder pre-warm, multi-root scan, orphan detection, parallel thumbnail generation, batched CLIP embedding, and cosine-index repopulation + persistence. Runs on a dedicated thread spawned from the Tauri `setup` callback (or from any command that mutates the root list) and emits structured `IndexingProgress` events so the frontend `IndexingStatusPill` renders live progress.

This system is what made the Phase 5 transition from "blocking pre-Tauri startup" to "window opens immediately and progress shows in the UI" possible.

## Boundaries / Ownership

- **Owns:** the pipeline lifecycle, single-flight gating (`AtomicBool`), the `IndexingProgress` event payload shape, the per-phase tracing instrumentation (`pipeline.scan_phase`, `pipeline.thumbnail_phase`, `pipeline.encode_phase`, `pipeline.cosine_repopulate`).
- **Does not own:** any SQL (delegates to `db/`), any CLIP math (delegates to `encoder` + `encoder_text`), any cosine retrieval (delegates to `cosine/index`), the watcher itself (delegates to `watcher.rs`).
- **Public API:** `IndexingState::new()`, `try_spawn_pipeline(app, state, db_path, cosine_index) -> Result<(), IndexingError>`, `IndexingProgress { phase, processed, total, message }`, `Phase` enum (kebab-case serialised).

## Current Implemented Reality

### Spawn lifecycle

```rust
// commands::roots::set_scan_root + add_root, lib.rs setup callback,
// watcher::start callback all call:
indexing::try_spawn_pipeline(app, indexing_state.clone(), db_path, cosine_index.clone())?;
```

The function does an atomic `compare_exchange(false, true)` on `IndexingState.is_running`. On failure (a pipeline already in flight) it returns `Err(IndexingError::AlreadyRunning)` without spawning anything. Callers map this to `ApiError::Internal(...)` for the IPC return.

On success the spawned thread:

1. Wraps the body in a `RunningGuard(Arc<IndexingState>)` whose `Drop` clears the bool — even a panic in the pipeline body cannot leave `is_running = true` and lock out future runs.
2. Calls `run_pipeline_inner(&app, &db_path, &cosine_index)` which is fully `tracing::instrument`-ed.
3. On `Err(_)` from the inner body, emits `Phase::Error` with the message string. The user sees the error in the indexing pill and can retry by switching folders or restarting.

### Phase ordering

```text
Phase::ModelDownload (from cache check + downloads)
    ──► Cosine cache load attempt (NOT a phase event — silent)
    ──► download_models_if_missing(progress_cb)
        Progress callback emits Phase::ModelDownload events with
        bytes processed / bytes total + filename in message.
    ──► Pre-warm text encoder if model + tokenizer exist (NOT a phase event)

Phase::Scan
    ──► Open second ImageDatabase (rusqlite supports concurrent connections; WAL keeps it cheap)
    ──► db.list_roots() → filter enabled
    ──► For every enabled root:
            ImageScanner::scan_directory(root_path) → Vec<String>
            collect into all_paths + paths_per_root
    ──► For every (path, root_id):
            db.add_image(path, Some(root_id))  — INSERT OR IGNORE
            emit Phase::Scan(processed, total) every 100 images
    ──► For every enabled root:
            db.mark_orphaned(root_id, alive_paths_for_that_root) — diff in Rust + chunked UPDATE
    ──► emit Phase::Scan(total, total) — final tick

Phase::Thumbnail
    ──► db.get_paths_to_root_ids() → HashMap<path, Option<root_id>>  (single SELECT, audit fix)
    ──► db.get_images_without_thumbnails() → Vec<ImageData>
    ──► rayon par_iter:
            ThumbnailGenerator::generate_thumbnail(path, image.id, root_id)
            db.update_image_thumbnail(image.id, &thumb_path, w, h)
            emit Phase::Thumbnail every ~25 thumbnails (coalesced via AtomicUsize bucket)

Phase::Encode (runs each available encoder family in sequence)
    1. CLIP via run_clip_encoder (clip_vision.onnx)
       ──► db.get_images_without_embeddings() → Vec<ImageData>
       ──► For every chunk of 32:
               encoder.encode_batch(&[Path]) → Vec<Vec<f32>>
               db.upsert_embeddings_batch("clip_vit_b_32", &batch_rows, false)
                       ↑ R1 — single BEGIN IMMEDIATE transaction per chunk.
                         legacy_clip_too=false (R8 dropped the legacy column write).
               db.checkpoint_passive()  ← R3 — drain WAL between batches
               emit Phase::Encode(processed, total) labelled "Encoding (CLIP)"
       ──► On first batch: emit "preprocessing_sample" diagnostic
       ──► At end of pass: emit "encoder_run_summary" diagnostic
                            { encoder_id, attempted, succeeded, failed,
                              elapsed_ms, mean_per_image_ms, failed_sample[≤10] }

    2. SigLIP-2 image branch via run_trait_encoder("siglip2_base", ...)  (if siglip2_vision.onnx exists)
       ──► db.get_images_without_embedding_for("siglip2_base")
       ──► same chunk loop, same R1 batch transaction + R3 checkpoint between batches

    3. DINOv2-Base via run_trait_encoder("dinov2_base", ...)  (if dinov2_base_image.onnx exists)
       ──► db.get_images_without_embedding_for("dinov2_base")
       ──► same chunk loop, same R1 batch transaction + R3 checkpoint between batches

    The encoder ORDER inside Phase::Encode honours the user's `priority_image_encoder`
    setting. If the user picked DINOv2 in the picker, DINOv2 runs FIRST so its embeddings
    land in the DB ASAP; the cosine cache hot-populates for that encoder as soon as its
    pass finishes (rather than waiting for all three encoders).

    Each encoder family is independently fail-soft — a missing model file or a session-creation error
    skips that encoder's pass with `warn` and continues to the next family. The encoder_run_summary
    diagnostic captures per-batch failures (e.g., one corrupted image in a batch of 32) in failed_sample
    so the user can identify them in the on-exit profiling report without having to grep tracing logs.

(cosine repopulate — NOT a separately-named phase; happens between Encode and Ready)
    ──► cosine.populate_from_db(&database)
    ──► cosine.save_to_disk()       ← writes Library/cosine_cache.bin

Phase::Ready (db.get_all_images().len() in the message)
```

Source: `indexing.rs:170-475`. The phase enum is serialised kebab-case (`#[serde(rename_all = "kebab-case")]`) so the frontend `useIndexingProgress` hook keys on `"model-download" | "scan" | "thumbnail" | "encode" | "ready" | "error"`.

### Single-flight semantics

The `AtomicBool` guarantee is "at most one pipeline running at any time across the whole app." Why:

- Watcher events arrive in 5s-debounced batches (`watcher.rs`). A bulk file drop produces one event, but two near-simultaneous user actions (set_scan_root + watcher event) could otherwise spawn two pipelines. Single-flight makes both safe — the second spawn returns `Err(AlreadyRunning)` and is silently coalesced.
- The DB writes are idempotent under WAL (`INSERT OR IGNORE`, `UPDATE WHERE id = ?`), so two concurrent pipelines wouldn't corrupt anything — they'd just waste CPU.
- The cosine cache `save_to_disk` is the most write-sensitive step (overwrites `cosine_cache.bin`); two simultaneous pipelines would race and one would lose. Single-flight prevents this.
- The on-spawn call site is consistent across `set_scan_root`, `add_root`, the watcher debounce callback, and the lib.rs setup callback — every trigger goes through `try_spawn_pipeline`.

### Cosine cache fast-path

Step 0 of `run_pipeline_inner`:

```rust
let db_path_buf = std::path::PathBuf::from(db_path);
if let Ok(mut idx) = cosine_index.lock() {
    if idx.cached_images.is_empty() {
        idx.load_from_disk_if_fresh(&db_path_buf);
    }
}
```

If `Library/cosine_cache.bin` is fresher than `Library/images.db` (compared via mtime), the populated `cached_images` vec is loaded directly from disk via bincode. The user can run similarity / semantic queries within milliseconds of app launch even before the rest of the pipeline finishes. If the cache is stale, the explicit populate_from_db at the end of the pipeline rebuilds it.

### Pre-warm text encoder

```rust
let text_encoder_state: tauri::State<'_, TextEncoderState> = app.state::<TextEncoderState>();
let lock_result = text_encoder_state.encoder.lock();
if let Ok(mut lock) = lock_result {
    if lock.is_none() {
        match TextEncoder::new(&model_path, &tokenizer_path) {
            Ok(encoder) => *lock = Some(encoder),
            Err(e) => warn!("text encoder pre-warm failed: {e}"),
        }
    }
}
```

Pre-warm means the user's first semantic search doesn't pay 1-2s of model-load latency. The lazy-init path in `commands::semantic` is preserved for the case where pre-warm failed (e.g., model missing during download). Today this initialises `ClipTextEncoder` from `clip_text.onnx` + `clip_tokenizer.json`; when the SigLIP-2 text-branch picker dispatch lands, the pre-warm logic will need to either pre-warm both or pick based on the user's `textEncoder` preference.

### Model download UX

`model_download::download_models_if_missing(progress_cb)` is wrapped in a closure that:
- Receives `(processed_bytes, total_bytes, current_file: Option<&str>)`
- Builds a human-readable message ("Downloading model_image.onnx — 245 / 1153 MB")
- Calls `emit(app, Phase::ModelDownload, processed, total, msg)`

The progress callback is the only `Phase::ModelDownload` event source. If models already exist on disk (subsequent launches), the download is skipped silently and no events fire — the pipeline jumps straight to the pre-warm.

## Key Interfaces / Data Flow

### Inputs

| Source | Provides |
|--------|----------|
| `lib.rs::run::setup` | First call (`try_spawn_pipeline` at app startup) |
| `commands::roots::set_scan_root` | After `wipe_images_for_new_root` + `add_root` for the new path |
| `commands::roots::add_root` | After `db.add_root(path)?` for an additional root |
| `watcher::start` (via debounce callback) | Whenever filesystem changes are debounced |
| `db: ImageDatabase` (constructed inside the thread) | Every read + write the pipeline does |
| `paths::models_dir()` | Where to look for ONNX files |
| `paths::thumbnails_dir()` (via `thumbnails_dir_for_root`) | Where to write thumbnails |

### Outputs

| Destination | What |
|-------------|------|
| `app.emit("indexing-progress", &payload)` | Per-phase progress payloads — see below |
| Database `images` table | INSERT OR IGNORE per scanned path; UPDATE thumbnail_path/width/height; UPDATE embedding |
| Database `images` table (orphan column) | UPDATE orphaned = 0/1 per `mark_orphaned` |
| Filesystem `Library/thumbnails/root_<id>/thumb_<id>.jpg` | One JPEG per image |
| Filesystem `Library/models/*.onnx`, `tokenizer.json` | Downloaded if missing |
| Filesystem `Library/cosine_cache.bin` | Written via `cosine.save_to_disk()` after every successful encode pass |
| `CosineIndexState.index` (Arc<Mutex<...>>) | Populated via `cosine.populate_from_db(&database)` |
| `TextEncoderState.encoder` | Pre-warmed if model files exist |

### `IndexingProgress` payload

```rust
#[derive(Serialize, Clone, Debug)]
pub struct IndexingProgress {
    pub phase: Phase,            // serialised kebab-case
    pub processed: usize,
    pub total: usize,            // 0 = indeterminate
    pub message: Option<String>,
}
```

Wire JSON example (model-download mid-flight):

```json
{
  "phase": "model-download",
  "processed": 245803520,
  "total": 1153023488,
  "message": "Downloading model_image.onnx — 234 / 1099 MB"
}
```

Frontend `useIndexingProgress` hook subscribes to the `"indexing-progress"` event and updates React state; `IndexingStatusPill` renders accordingly.

## Implemented Outputs / Artifacts

- 6 phases visible to the frontend: scan, model-download, thumbnail, encode, ready, error.
- One emit per phase boundary plus periodic throttled emits within long phases (every 100 images during scan, every 25 thumbnails, after each encode batch).
- Every successful pipeline run leaves: a complete `images` table (paths + thumbnails + embeddings), a populated cosine cache (in-memory + on-disk), and a pre-warmed text encoder.
- 7 unit tests covering single-flight semantics, IndexingError display, kebab-case Phase serialisation. Source: `indexing.rs:497-601`.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Model download is not resumable | Network failure mid-1 GB download | The partial file is left on disk; the next launch sees it and re-downloads from scratch. Verify by checking whether `download_to_file` writes to a `.part` file (it does — gitignore covers `*.part`) and renames on completion. |
| Pipeline panic with cosine mutex held | Any panic inside `populate_from_db` or `save_to_disk` while holding the cosine Mutex | The `Arc<Mutex<CosineIndex>>` becomes poisoned; every subsequent `commands::similarity` / `commands::semantic` call returns `ApiError::Cosine("mutex poisoned: ...")`. Recovery requires app restart. |
| Watcher rebuild on root change is missing | Adding a root after launch then dropping a file in it | The new root is not watched until the next app launch. The `add_root` command's `try_spawn_pipeline` covers the immediate rescan, so the file appears in the catalogue, but subsequent additions to that root require a manual refresh via `set_scan_root` or restart. |
| Concurrent pipeline + UI cosine query | UI semantic-search during the cosine_repopulate phase | Both contend for the same Arc<Mutex<CosineIndex>>. The query waits a few hundred ms while the populate finishes. Not user-visible in practice. |
| Encode phase is sequential despite the rest of the pipeline being parallel | Large libraries (10k+ images) with ~30ms per CLIP inference | A 10k-image library takes ~5 minutes of encode wallclock. Parallelism would help; this is on the active task list (LifeOS Quick Notes — pipeline parallelism #74). |
| `mark_orphaned` chunks at 500 ids per UPDATE | Libraries with >500 newly-orphaned images in one rescan | Multiple sequential UPDATEs run; not parallelised. The chunking is to stay under SQLite's parameter limit, not for performance. |
| Empty roots (configured root no longer exists on disk) | User pointed at a folder, then deleted/moved it | The pipeline logs a `warn` per missing root and continues with whatever exists. If every root is missing, `Phase::Ready` is emitted with `total = 0` and an empty-state message. |

## Partial / In Progress

None — the pipeline as written is feature-complete for the current scope. Plans for parallelism live in `plans/pipeline-parallelism-and-stats-ui.md`.

## Planned / Missing / Likely Changes

- **Pipeline parallelism**: overlap thumbnail + encode phases via independent worker threads. The DB acts as the queue (rows missing thumbnails vs rows missing embeddings); each worker polls and processes. Requires careful ordering to avoid encoding before thumbnail confirms the file is decodable. Tracked in `plans/pipeline-parallelism-and-stats-ui.md`.
- **Pipeline stats UI**: surface `db::get_pipeline_stats` (already implemented backend-side) in the Settings drawer or status pill — counts of total / with-thumbnail / with-embedding / orphaned. Tracked in the same plan.
- **Watcher rebuild on root changes**: drop the existing `WatcherHandle` and re-call `watcher::start` after `add_root` / `remove_root` / `set_root_enabled`. Today's gap is documented in `systems/watcher.md`.
- **Cancellation token for cooperative pipeline cancel**: today the only way to abort a running pipeline is to wait for it. A future "cancel-and-restart on rapid root switches" UX would need a `Arc<AtomicBool>` cancel flag checked between phases.
- **Resumable downloads**: `model_download` could honour HTTP `Range` headers to resume partial downloads instead of restarting from byte 0.

## Durable Notes / Discarded Approaches

- **Single-flight via `AtomicBool` + RAII guard, not via channel-based queueing.** The trade-off: a queue would let bursts accumulate and eventually all be processed; single-flight coalesces them into one rescan. For filesystem watch events this is the right choice — the user wants "the latest state of the disk reflected in the catalogue," not "every intermediate state replayed." For user-driven multi-root operations (clicking Add three times in a row) it's also right because each `add_root` does its own `db.add_root` synchronously before spawning the pipeline; the pipeline just needs to run once after all the roots land.
- **Why a second `ImageDatabase` connection in the indexing thread, not borrowing the Tauri-managed one?** Because the Tauri-managed `ImageDatabase` lives behind `tauri::State<'_, ImageDatabase>` which is only accessible from inside command handlers, not from a background thread. Opening a second connection is the simplest way; WAL means the contention is bounded to a few μs per write.
- **The cosine cache is invalidated by file mtime, not by an explicit version number.** This works because every successful encode pass touches the DB (writing embeddings) right before saving the cache. If a future change writes embeddings in a different sequence, the freshness check would need updating. See `cache.rs::load_from_disk_if_fresh`.
- **Why coalesce thumbnail-progress emits to ~25 per emit?** The frontend pill re-renders on every event; firing one per thumbnail (1500+ events for a typical first run) caused noticeable jank in the UI. The bucket-based throttling lands ~60 events for a 1500-thumbnail run while keeping the bar smooth.
- **Pre-warm + lazy init coexistence is intentional.** Pre-warm covers the common case (user starts the app, models exist, by the time they search the encoder is loaded). Lazy init covers the edge cases (pre-warm failed silently because models were still downloading; user upgraded the binary and the model files have been deleted; model files corrupt). The double init protection costs nothing because the lock check `if encoder_lock.is_none()` short-circuits when pre-warm succeeded.

## Obsolete / No Longer Relevant

The previous architecture where indexing ran inside `main()` before Tauri started (visible as "blank window for 30 seconds, then the app appears with everything ready") is gone. `main.rs` now does only `db_path` + `ImageDatabase::new` + `initialize`; the heavy lifting moved into the spawned thread.
