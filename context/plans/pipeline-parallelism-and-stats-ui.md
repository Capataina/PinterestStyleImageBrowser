# Pipeline parallelism + stats UI

## Header

Two related but separable next-session tasks. Both extend the existing indexing pipeline and surface its progress more richly to the user.

- **Status:** RETROSPECTIVE — both tasks landed in earlier sessions (parallelism via the `run_clip_encoder` / `run_trait_encoder` per-encoder loop + Phase 5 RRF using all three concurrently; stats UI as the StatsSection in Settings, commit `8c55aa4`). Kept here as a record of what was originally scoped and how the implementation diverged. The actual canonical home for current pipeline reality is `systems/indexing.md`; for the stats UI, `systems/frontend-state.md` and `systems/database.md`'s `get_pipeline_stats` description.
- **Date:** 2026-04-26 (created during upkeep-context Restructure pass)
- **Trigger:** user-flagged as the next two pieces of work in `Quick Notes.md`. Captured in this plan so they have a structured home before they're touched in code.

## Why these belong together

Both modify the indexing pipeline's relationship with its consumers:
- **#74 (parallelism)** changes how phases relate to *each other* inside the backend — overlapping thumbnail and encode work via independent worker threads.
- **#75 (stats UI)** changes how the pipeline relates to the *frontend* — surfacing per-stage counts beyond the binary "in progress / ready" pill.

Together they make the indexing pipeline more visible AND more efficient.

## Implementation Structure

### Task #74: Pipeline parallelism

Today's pipeline is sequential: scan completes → orphan-mark → thumbnails complete → encode begins. Wallclock is dominated by encode (sequential CLIP inference at ~30 ms per image; ~5 minutes for 10k images), but the thumbnail phase has finished by then so the GPU/ANE could start encoding earlier.

Proposed shape: the **DB acts as a queue**. Two long-running worker threads poll for work:

- **Thumbnail worker**: polls `db.get_images_without_thumbnails()` periodically, processes a chunk via rayon, sleeps. When the result is empty for N consecutive polls, the worker exits.
- **Encode worker**: polls `db.get_images_without_embeddings()` periodically, processes a chunk via the CLIP encoder, sleeps. Same exit condition.

The scan phase still runs first (and is still triggered by the same try_spawn_pipeline path) so DB rows exist before workers start. Once scan emits `Phase::Scan` final, the workers begin polling. They terminate independently when their work queue is empty.

Concurrency model:
- Each worker holds its own `ImageDatabase` instance (third + fourth SQLite connection; WAL keeps reads non-blocking).
- The `IndexingState.is_running` AtomicBool now needs to track multiple workers — could become a count or a richer state. Single-flight semantics still apply at the pipeline level (no parallel pipelines), but within a pipeline the workers are independent.
- Encode worker waits for at least one thumbnail to confirm the file is decodable — proposed: encode polls `WHERE thumbnail_path IS NOT NULL AND embedding IS NULL`. This means encode lags behind thumbnail by one chunk minimum, which is acceptable.
- Cosine repopulate runs once after BOTH workers exit (their absence of work is the signal that "everything that can be encoded has been encoded").

Risk considerations:
- A bad image that fails thumbnail generation indefinitely could starve encode. Mitigation: thumbnail failure marks the row with empty `thumbnail_path = ''` so encode's `WHERE thumbnail_path IS NOT NULL` includes it for retry attempt; or the encode polling treats any non-NULL as eligible.
- Multiple ONNX sessions across workers would be wasteful (1 GB RAM each); the encode worker is the only ONNX consumer and should be single-instance per pipeline.
- Watcher debouncing means concurrent rescans can pile up; single-flight at the pipeline level still prevents this.

### Task #75: Pipeline stats UI

Backend already exists (`db/images_query.rs::get_pipeline_stats`):

```rust
pub struct PipelineStats {
    pub total_images: i64,
    pub with_thumbnail: i64,
    pub with_embedding: i64,
    pub orphaned: i64,
}
```

Single SELECT, four COUNT-style aggregates, returned as a serializable struct. Already covered by 3 unit tests.

What's missing:

- Tauri command wrapper (`commands::images::get_pipeline_stats`) returning `Result<PipelineStats, ApiError>`.
- Frontend service wrapper (`services/images.ts::getPipelineStats`).
- React Query hook (`useQuery({ queryKey: ["pipelineStats"], queryFn: getPipelineStats, refetchInterval: 1000 })` — polls during indexing, idle when complete).
- UI surface — most natural in the Settings drawer's Folders section (showing per-root counts when a root is highlighted), or as a secondary line in the IndexingStatusPill (showing aggregate counts when active).

Possible UX shapes:

1. Always-on counter line below the status pill: `1234 / 1500 thumbs, 980 / 1500 encodings, 12 orphans`.
2. Hover-only detail panel on the status pill.
3. Settings drawer addition: a "Library status" section above the Folders list.

Recommendation: Settings drawer addition. The status pill is for transient state (active indexing); the drawer is the natural home for "how is my library doing right now" answers.

## Algorithm / System Sections

### Worker-thread coordination (#74)

```text
indexing.rs::run_pipeline_inner (revised):

1. Cosine cache load            (unchanged)
2. Model download               (unchanged)
3. Text encoder pre-warm        (unchanged)
4. Open DB connection           (unchanged)
5. Phase::Scan + orphan mark    (unchanged)
6. emit Phase::Scan(total, total)
7. SPAWN: thumbnail worker thread + encode worker thread
8. Workers poll independently:
     thumbnail: get_images_without_thumbnails → rayon chunk
     encode:    get_images_without_embeddings AND thumbnail_path NOT NULL → batch chunk
   Each emits per-batch progress events with its own phase enum value.
9. JOIN both workers (workers exit on empty-queue threshold)
10. Cosine repopulate + save_to_disk    (unchanged, runs after both workers exit)
11. emit Phase::Ready                   (unchanged)
```

New phase enum variants might be needed:
- `Phase::Thumbnail` keeps its current meaning but emits from the thumbnail worker
- `Phase::Encode` keeps its current meaning but emits from the encode worker
- A composite `Phase::ThumbnailAndEncode` could be added if the UI wants a combined indicator instead of two competing progress bars

### get_pipeline_stats wiring (#75)

```text
backend:
  commands/images.rs:
    #[tauri::command]
    #[tracing::instrument(name = "ipc.get_pipeline_stats", skip(db))]
    pub fn get_pipeline_stats(db: State<'_, ImageDatabase>)
        -> Result<PipelineStats, ApiError> { Ok(db.get_pipeline_stats()?) }
  
  lib.rs::run() invoke_handler! → add get_pipeline_stats

frontend:
  services/images.ts:
    export async function getPipelineStats(): Promise<PipelineStats> {
      return invoke("get_pipeline_stats");
    }
  
  queries/usePipelineStats.ts (new):
    export function usePipelineStats() {
      const { data: progress } = useIndexingProgress();
      const indexing = progress && progress.phase !== "ready" && progress.phase !== "error";
      return useQuery({
        queryKey: ["pipelineStats"],
        queryFn: getPipelineStats,
        refetchInterval: indexing ? 1000 : false,
        staleTime: indexing ? 0 : Infinity,
      });
    }
  
  components/settings/FoldersSection.tsx (extend):
    const { data: stats } = usePipelineStats();
    render the four counts inline
```

## Integration Points

| Existing system | What changes |
|----------------|--------------|
| `indexing` | Major rewrite of `run_pipeline_inner` — phase ordering becomes parallel + worker-based |
| `database` | No schema change; `get_pipeline_stats` already implemented |
| `tauri-commands` | One new command (`get_pipeline_stats`) |
| `frontend-state` | New hook (`usePipelineStats`) + new query key |
| `frontend / settings` | Extension to FoldersSection (or new section) |
| `profiling` | Spans for new worker threads (`pipeline.thumbnail_worker`, `pipeline.encode_worker`) |
| `cosine-similarity` | No change — repopulate still runs once at the end |
| `watcher` | No change — single-flight at the pipeline level still applies |

## Debugging / Verification

- Run with `--profiling` to confirm the workers actually overlap in the perf report (their spans should interleave, not be sequential as today).
- Verify `cargo test` still passes — single-flight tests + serialisation tests should be unaffected; new tests cover the worker-coordination state machine.
- Verify `Phase::Ready` only emits after both workers finish (not after either one alone).
- Verify no double-encode: the `WHERE embedding IS NULL` filter should prevent re-encoding work the previous run completed.
- Verify the stats UI updates live during indexing: open the Settings drawer with the indexing pipeline mid-flight; counters should tick up at ~1 Hz.
- Verify the stats UI is idle when complete: closing + reopening the drawer after `Phase::Ready` should not re-fetch.
- Verify the watcher → re-spawn path still works: drop a file in a watched folder mid-indexing; the second pipeline-spawn should be coalesced via single-flight.

## Completion Criteria

- [ ] Indexing pipeline has independent thumbnail + encode workers; perf report shows they overlap.
- [ ] Encode worker correctly waits for thumbnail before encoding (no encode of un-thumbnailed rows).
- [ ] Single-flight at the pipeline level still prevents concurrent pipeline runs.
- [ ] `get_pipeline_stats` Tauri command exists and is registered.
- [ ] `usePipelineStats` hook polls at 1 Hz during indexing, stops when idle.
- [ ] Settings drawer shows live counts (or alternative UI surface chosen).
- [ ] Existing single-flight + IndexingProgress tests still pass.
- [ ] New tests cover the worker-coordination state machine.
- [ ] Documentation updated: `systems/indexing.md` reflects the parallel layout; `systems/database.md` notes the new IPC consumer of `get_pipeline_stats`.
