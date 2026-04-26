# watcher

*Maturity: working*

## Scope / Purpose

Filesystem-watching layer for live catalog integrity. Recursively watches every enabled root and triggers an incremental rescan via the indexing pipeline whenever files change on disk. Debounces noisy event streams so a "drop 100 photos" action produces one rescan, not one per file. Runs once at app startup and lives for the lifetime of the process.

## Boundaries / Ownership

- **Owns:** the `notify-debouncer-mini` debouncer setup, the per-root `watcher.watch(path, RecursiveMode::Recursive)` calls, the debounce-event closure that re-spawns the indexing pipeline.
- **Does not own:** the indexing pipeline itself (delegates to `indexing::try_spawn_pipeline`), the single-flight gating (`indexing::IndexingState` provides that), the rescan logic itself.
- **Public API:** `start(app, paths_to_watch, db_path, indexing_state, cosine_index) -> Option<WatcherHandle>`, type alias `WatcherHandle = Debouncer<notify::RecommendedWatcher>`.

## Current Implemented Reality

### Setup at app launch

```rust
// lib.rs::run::setup
let watch_paths: Vec<PathBuf> = db.list_roots()
    .unwrap_or_default()
    .into_iter()
    .filter(|r| r.enabled)
    .map(|r| PathBuf::from(r.path))
    .filter(|p| p.exists())
    .collect();

let handle = watcher::start(
    app_handle,
    watch_paths,
    db_path,
    indexing_state,
    cosine_index,
);
if let Ok(mut slot) = watcher_state.lock() {
    *slot = handle;
}
```

The handle is stashed in `Arc<Mutex<Option<WatcherHandle>>>` Tauri-managed state. Dropping the handle cancels every watch — the wrapper exists so the watcher lives the lifetime of the app process and is not garbage-collected.

### Debounce semantics

```rust
new_debouncer(
    Duration::from_secs(5),
    move |result: DebounceEventResult| {
        let _span = tracing::info_span!("watcher.event").entered();
        match result {
            Ok(events) => { /* trigger rescan */ }
            Err(e) => { warn!("watcher debounce error: {e:?}"); }
        }
    },
)
```

5 second debounce was chosen because raw notify events on macOS fire dozens of times per "save" (every metadata change, every fsync). 5s collapses a typical bulk add (dropping 100 photos into a folder, batched by Finder) into a single rescan trigger.

### Rescan trigger

Inside the closure:

```rust
let _ = indexing::try_spawn_pipeline(
    app_for_handler.clone(),
    indexing_state_for_handler.clone(),
    db_path_for_handler.clone(),
    cosine_index_for_handler.clone(),
);
```

The `let _ = ...` is intentional: if a pipeline is already in flight (`Err(IndexingError::AlreadyRunning)`), the second event is silently coalesced. Single-flight in `indexing` does the right thing here — the user sees a single rescan covering everything that changed in the debounce window, not stacked reindexes.

### Manual span wrapping

The debounce closure can't carry `#[tracing::instrument]` (it's not a top-level function), so the closure body opens a manual `tracing::info_span!("watcher.event").entered()` for per-batch timing. Each debounce-batch event handler shows up in the perf report as one `watcher.event` span; the surrounding `watcher.start` span (added via `#[tracing::instrument]` on the public function) covers the initial debouncer construction.

## Key Interfaces / Data Flow

### Inputs

| Source | Provides |
|--------|----------|
| `lib.rs::run::setup` | Initial root list (enabled roots that exist on disk) |
| `db.list_roots()` (read once at startup) | Where to watch |
| `notify::RecommendedWatcher` (per platform: `FSEvents` on macOS, `inotify` on Linux, `ReadDirectoryChangesW` on Windows) | Raw filesystem events |

### Outputs

| Destination | What |
|-------------|------|
| `indexing::try_spawn_pipeline(...)` | Rescan trigger; output is the indexing pipeline's progress events |

### State held

- `Arc<IndexingState>` — shared with the indexing pipeline (single-flight)
- `Arc<Mutex<CosineIndex>>` — shared with the indexing pipeline (cache)
- `String` — the db_path for spawning a fresh `ImageDatabase` inside the pipeline thread
- `AppHandle` — for the `try_spawn_pipeline` call (the spawned thread emits events)

## Implemented Outputs / Artifacts

- One `WatcherHandle` per app process (or `None` if no roots are enabled at launch).
- Tracing spans `watcher.start` (one per launch) and `watcher.event` (one per debounce batch) for the perf report.
- No DB writes, no file writes — the watcher is purely a trigger.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| **Watcher does NOT auto-reconfigure when roots change.** | `add_root` / `remove_root` / `set_root_enabled` after launch | New roots are not watched until the next launch. Removing a root leaves a dangling watch on the path. The indexing pipeline that those commands trigger covers the immediate state, but subsequent file changes in newly-added roots aren't picked up. Documented as a planned change. |
| 5s debounce window is global per debouncer | Two unrelated bulk operations on different roots that happen to overlap in time | Both batches collapse into one rescan trigger. Not a correctness issue (the rescan covers all roots anyway), just a coalescing of unrelated work. |
| Debouncer can fail to initialise on some platforms | `notify::RecommendedWatcher` returns `Err` (e.g., out of inotify watches on Linux with too many recursive subdirectories) | `start` returns `None`; the slot stays empty; the app works without live integrity. The user can still trigger rescans by switching folders or restarting. |
| Permission errors per-root are swallowed with a warn | `watcher.watch(path, RecursiveMode::Recursive)` returns `Err` | The other roots still get watched. The unwatched root logs a warn but does not block startup. |
| Symlink behaviour | A root that contains symlinks pointing into another root | Could cause double-event delivery and a single rescan covering both — harmless. Could also cause infinite descent if a symlink loops; `notify` does not loop-protect, but `ImageScanner` uses `read_dir` which also does not. Untested. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Rebuild watcher on root changes.** After `add_root` / `remove_root` / `set_root_enabled` succeeds, drop the old `WatcherHandle` from the Mutex slot and call `watcher::start(...)` again with the new enabled-root list. Today's gap is acknowledged in the source comment block at the top of `watcher.rs`. Estimated effort: small — the Mutex<Option<...>> is the right shape, just needs the swap.
- **Per-root debounce windows.** A single 5s debounce works for the common case but a heavy ingest (dropping 1000 photos) could produce one rescan trigger 5s after the last file lands; 5s is a long time to wait for "I just added a photo and want to see it." Adaptive debounce (smaller window for small bursts, larger for large) is possible but not currently warranted.
- **Filter events by extension.** Notify reports every metadata change including `.DS_Store`, `Thumbs.db`, etc. Today the filtering happens in the indexing pipeline (the scanner ignores non-image extensions). A pre-filter in the watcher closure could short-circuit the spawn-then-discard path if no image extensions changed.

## Durable Notes / Discarded Approaches

- **`notify-debouncer-mini` chosen over raw `notify`.** The mini debouncer is purpose-built for this exact use case ("collapse N events fired within W ms into one"). The full `notify-debouncer-full` adds file metadata that the rescan-everything pipeline doesn't need.
- **5s is not a tuning parameter.** It was chosen empirically: long enough to coalesce a Finder bulk-copy operation (which fires events spread over 1-3 seconds), short enough that a quick "drop one file then come back to the app" feels responsive. Reducing it would cause more rescans for the same workload; increasing it would make new images take longer to appear.
- **The `let _ = try_spawn_pipeline(...)` pattern is intentional.** The single-flight gate is the upstream contract; the watcher trusts it to do the right thing. Surfacing the `Err(AlreadyRunning)` would just trigger a UI toast that the user can't act on.

## Obsolete / No Longer Relevant

The pre-Phase-7 model had no watcher at all — the only way to refresh the catalogue was to restart the app or switch folders. Replaced by this system.
