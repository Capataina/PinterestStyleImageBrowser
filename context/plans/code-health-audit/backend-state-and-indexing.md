# Backend State And Indexing — Code Health Findings

**Systems covered:** Tauri state wiring, root commands, cosine cache state, indexing pipeline
**Finding count:** 2 findings (1 high, 1 medium)

## Known Issues and Active Risks

### Cosine Cache Can Stay Marked Current After Being Cleared
- [ ] Make cosine cache invalidation reset both `cached_images` and the loaded-encoder marker, then enable the ignored diagnostic test.

**Category:** Known Issues and Active Risks
**Severity:** high
**Effort:** small
**Behavioural Impact:** none for the intended fix; it restores the behaviour already described by the comments.

**Location:**
- `src-tauri/src/lib.rs:57` — `CosineIndexState::ensure_loaded_for`
- `src-tauri/src/commands/roots.rs:51` — `set_scan_root`
- `src-tauri/src/commands/roots.rs:133` — `remove_root`
- `src-tauri/src/commands/roots.rs:152` — `set_root_enabled`
- `src-tauri/src/commands/similarity.rs:133` — `get_tiered_similar_images`
- `src-tauri/tests/cosine_cache_invalidation_diagnostic.rs:15` — ignored diagnostic reproducer

**Current State:**
Root commands clear the shared `CosineIndex.cached_images` vector when the root set changes, but they do not reset `CosineIndexState.current_encoder_id`. `ensure_loaded_for` checks only the marker; if it already matches the requested encoder, it returns before taking the index lock. That means a subsequent image-similarity query can see an empty cache that is still labelled as `clip_vit_b_32`, log "encoder probably has no embeddings yet", and return zero results even though embeddings exist in SQLite.

**Proposed Change:**
Centralise cache invalidation behind a small helper on `CosineIndexState`, for example `invalidate_cache()`, that takes locks in the existing order (`current_encoder_id`, then `index`), clears `cached_images`, and clears the marker. Alternatively, make `ensure_loaded_for` treat `(marker matches, cache empty)` as stale and repopulate. Prefer the helper because root commands are the invalidation source and should express the full invariant directly.

After the fix, remove the `#[ignore]` attribute from `src-tauri/tests/cosine_cache_invalidation_diagnostic.rs` and run the test normally.

**Justification:**
The project's own comments say root-command cache clearing should "let next-query populate from the remaining DB rows", but the implementation short-circuits before that populate path. Tauri's state-management model supports shared mutable state through `Mutex`; the issue here is not the use of shared state, it is that two pieces of state represent one invariant and can be updated independently. The diagnostic test demonstrates the failing invariant without needing Tauri runtime setup.

**Expected Benefit:**
Prevents false empty search results after root removal or enabled/disabled toggles. It also gives future cache invalidation work one helper to call, reducing the chance that new root/settings commands repeat the same half-invalidation.

**Impact Assessment:**
The fix should not change visible behaviour except removing the bug. Repopulating an empty cache from SQLite is the documented fallback path, and the same DB rows are already used by the normal encoder-switch path. The main edge case is a genuinely empty embeddings table; in that case repopulation still leaves the cache empty, which preserves current behaviour.

## Modularisation

### Split Indexing Pipeline Phases Behind The Existing Public API
- [ ] Split `src-tauri/src/indexing.rs` into private phase modules while keeping `try_spawn_pipeline` as the external entry point.

**Category:** Modularisation
**Severity:** medium
**Effort:** medium
**Behavioural Impact:** none if limited to movement and private helper extraction.

**Location:**
- `src-tauri/src/indexing.rs:178` — `run_pipeline_inner`
- `src-tauri/src/indexing.rs:557` — `run_encoder_phase`
- `src-tauri/src/indexing.rs:677` — `run_clip_encoder`
- `src-tauri/src/indexing.rs:766` — `run_trait_encoder`

**Current State:**
`indexing.rs` is 1,016 lines and owns several distinct responsibilities: model download progress, CLIP text prewarming, enabled-root scanning, orphan marking, thumbnail generation, encoder ordering, encoder execution, per-encoder cache hot-population, final cache save, event emission, and pipeline tests. The file is not chaotic, but it has crossed the point where a small change to one phase forces the reader to keep unrelated concurrency and cache details in working memory.

**Proposed Change:**
Keep `try_spawn_pipeline` and `Phase` in `indexing.rs`, but move private implementation into focused modules:

- `indexing/scan.rs` for enabled-root collection, path insertion, and orphan marking.
- `indexing/thumbnails.rs` for the rayon thumbnail worker and progress coalescing.
- `indexing/encoders.rs` for encoder order, CLIP/SigLIP/DINOv2 loops, and per-encoder summaries.
- `indexing/cache.rs` for hot-populate/final-save helpers shared with the cache invalidation fix.

Do not change pipeline ordering or external command contracts as part of the split.

**Justification:**
The SQLite WAL research supports the current high-level design: foreground reads and background writes can proceed concurrently under WAL, so the pipeline's parallel thumbnail/encoder shape is reasonable. The health issue is local navigability and edit blast radius, not the concurrency architecture. Splitting by phase makes each module testable in isolation and reduces the risk that cache-state changes accidentally perturb scan or thumbnail behaviour.

**Expected Benefit:**
Reduces the main indexing file from ~1,000 lines to a small orchestration surface plus phase modules. Future work such as watcher rebuilds, encoder batching, or cache invalidation can land in the relevant phase without re-reading the full pipeline.

**Impact Assessment:**
Pure movement/extraction has no observable behaviour change. The failure mode is accidentally changing the lock order around cosine cache hot-population; preserve the documented order (`current_encoder_id` before `CosineIndex`) and keep existing pipeline tests passing.
