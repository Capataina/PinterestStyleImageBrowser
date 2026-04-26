# cosine-similarity

*Maturity: comprehensive*

## Scope / Purpose

In-memory similarity index over the database's CLIP embeddings. Provides three retrieval modes: random-sampled top-N (diversity), strictly-sorted top-N (semantic search), and Pinterest-style tiered sampling (visual similarity). Constructed empty at app startup and populated either from the persistent on-disk cache (`Library/cosine_cache.bin`) at indexing-pipeline start or via a single-SELECT `populate_from_db(&ImageDatabase)` after every encode pass.

The module was previously a single `cosine_similarity.rs` of 860 lines. After the audit Modularisation finding it lives in `src-tauri/src/similarity_and_semantic_search/cosine/` with the original file kept as a 9-line `pub use cosine::*;` shim so every existing import path continues to resolve unchanged.

## Boundaries / Ownership

- **Owns:** the in-memory `Vec<(PathBuf, Array1<f32>)>` cache, the cosine math + NaN-aware comparator, the three retrieval methods, the reusable per-query scratch buffer, the persistent disk cache (load_from_disk_if_fresh + save_to_disk).
- **Does not own:** persistence canonicality (the embeddings themselves live in SQLite as BLOBs; the disk cache is a load-time optimisation), embedding generation (delegates to `clip-image-encoder`), exclude-path normalisation (caller passes `&PathBuf`).
- **Public API:** `CosineIndex::new()`, `add_image(path, embedding)`, `populate_from_db(&db::ImageDatabase)`, `cosine_similarity(a, b)` (associated fn — thin delegate to `math::`), `get_similar_images(emb, top_n, exclude_path)`, `get_similar_images_sorted(emb, top_n, exclude_path)`, `get_tiered_similar_images(emb, exclude_path)`, `save_to_disk()`, `load_from_disk_if_fresh(db_path: &Path)`. Plus `cached_images: Vec<...>` is `pub` so commands::roots can clear it directly.

## Current Implemented Reality

### Submodule layout

```
src-tauri/src/similarity_and_semantic_search/cosine/
├── mod.rs           — pub use index::CosineIndex; module declarations
├── math.rs          — cosine_similarity helper + score_cmp_desc (NaN-aware desc comparator) + 8 math tests
├── index.rs         — CosineIndex struct + new() + add_image + populate_from_db
│                       + populate_from_db_for_encoder + 3 retrieval methods
│                       + scratch buffer + select_nth_unstable_by partial sort
│                       + diagnostic emissions (cosine_cache_populated, embedding_stats,
│                         pairwise_distance_distribution, self_similarity_check)
├── diagnostics.rs   — 4 stateless helpers consumed by index.rs and commands::*:
│                       embedding_stats, pairwise_distance_distribution,
│                       self_similarity_check, score_distribution_stats
└── cache.rs         — save_to_disk / save_to_path / load_from_disk_if_fresh / load_from_path_if_fresh
                       + 5 cache disk-persistence tests; in a separate impl CosineIndex block

src-tauri/src/similarity_and_semantic_search/
└── cosine_similarity.rs  — 9-line shim: `pub use crate::similarity_and_semantic_search::cosine::*;`
```

The shim keeps every existing import path (e.g., `crate::similarity_and_semantic_search::cosine_similarity::CosineIndex`) working unchanged in `lib.rs`, `indexing.rs`, `watcher.rs`, and the integration test crate.

### Per-encoder population — `populate_from_db_for_encoder(&db, encoder_id)`

The cosine cache holds embeddings for ONE encoder at a time. When the user switches encoders in Settings, `CosineIndexState::ensure_loaded_for(&db, "siglip2_base")` checks the current encoder id, and if it differs, calls `populate_from_db_for_encoder` to wipe the cache and reload from `db.get_all_embeddings_for(encoder_id)`. Repopulate is fast because the embeddings are already on disk; only the new encoder's rows need DB → memory transfer.

For `clip_vit_b_32` specifically, if the new per-encoder embeddings table is empty (users who indexed before the per-encoder schema), the function falls back to the legacy `images.embedding` column via `get_all_embeddings()`. This back-compat will go away once the embedding-pipeline migration has run on every install.

Three diagnostics fire after every successful populate (when profiling is on — no-op otherwise):

- `cosine_cache_populated` — encoder_id, count, duration_ms
- `embedding_stats` — L2 norm summary, per-dim mean/std, NaN/Inf counts, samples
- `pairwise_distance_distribution` — 50-sample × 1225-pair cosine histogram across 11 buckets
- `self_similarity_check` — cosine(emb_0, emb_0), expected 1.0, with a `passes` boolean

These are the primary signals for "is this encoder's cache loaded with healthy embeddings?" — see `systems/profiling.md` § Domain diagnostics.

### `populate_from_db` — single SELECT, no second connection (legacy path)

```rust
pub fn populate_from_db(&mut self, db: &db::ImageDatabase) {
    let rows = match db.get_all_embeddings() {
        Ok(r) => r,
        Err(e) => { warn!("populate_from_db: get_all_embeddings failed: {e}"); return; }
    };
    self.cached_images.clear();
    self.cached_images.reserve(rows.len());
    for (_id, path, embedding) in rows {
        if embedding.is_empty() { continue; }
        self.cached_images.push((PathBuf::from(path), Array1::from_vec(embedding)));
    }
}
```

`cosine/index.rs:43-68`. Two audit fixes:

1. Takes `&db::ImageDatabase` (not a `db_path: &str`). The previous signature constructed a fresh `ImageDatabase` from a stored path string — a documented coupling smell that forced the cosine module to hold a duplicate connection. Now it borrows the existing one.
2. Calls `get_all_embeddings()` (single SELECT) instead of looping `get_image_embedding(id)` per image. ~30× faster for 1000+ image libraries.

The `CosineIndexState.db_path` field is preserved because the indexing pipeline still constructs its own `ImageDatabase` on the background thread (Tauri-managed `ImageDatabase` is only reachable from inside command handlers). The cosine module itself no longer reads `db_path`.

### Three retrieval modes

```rust
pub struct CosineIndex {
    pub cached_images: Vec<(PathBuf, Array1<f32>)>,
    pub(super) scratch: Vec<(usize, f32)>,    // reusable per-query buffer
}
```

| Mode | Method | Returns | Used by |
|------|--------|---------|---------|
| Diversity-sampled | `get_similar_images(emb, top_n, exclude_path)` | Sort by cosine desc, take top max(top_n, 20% of pool), randomly pick top_n from that pool | `commands::similarity::get_similar_images` |
| Strict-sorted | `get_similar_images_sorted(emb, top_n, exclude_path)` | Top top_n by cosine desc — exactly | `commands::semantic::semantic_search` |
| 7-tier | `get_tiered_similar_images(emb, exclude_path)` | 5 random per tier × 7 tiers (0-5%, 5-10%, 10-15%, 15-20%, 20-30%, 30-40%, 40-50%); deduplicated via `HashSet<usize>` | `commands::similarity::get_tiered_similar_images` |

The tiered method is the most product-thoughtful piece of the backend. The tier definitions live as a literal array; the within-tier randomness keeps the result feed fresh on repeated views while the deterministic tier structure keeps the visual coherence.

### Partial sort (audit Algorithm Optimisation finding)

```rust
// In get_similar_images_sorted (and the diversity-pool prefix in get_similar_images):
let len = self.scratch.len();
let k = top_n.min(len);
if k > 0 && k < len {
    self.scratch.select_nth_unstable_by(k - 1, score_cmp_desc);
}
let mut top: Vec<(usize, f32)> = self.scratch.iter().take(k).copied().collect();
top.sort_by(score_cmp_desc);   // re-sort the trimmed top-K only
```

Replaces the previous full sort + take(top_n) pattern (audit `c6551e2`). At n=10000, top_n=50 the diagnostic test (`src-tauri/tests/cosine_topk_partial_sort_diagnostic.rs`) measures **2.53× speedup** in debug. Set equivalence + order equivalence after re-sorting are pinned by the test so any future regression is caught.

### Reusable scratch buffer

The `scratch` field holds `(index_into_cached_images, similarity)` tuples. Cleared on entry to each retrieval method, capacity preserved across calls. Two wins:

1. The inner loop never clones a `PathBuf` (only the indices that survive into the final result get path-cloned at the end).
2. After the first warm query, allocations in the inner loop drop to zero — capacity is sufficient.

`pub(super)` visibility lets retrieval methods in `index.rs` access the buffer from sibling files without exposing it on the `CosineIndex` public API.

### Cosine math (in `math.rs`)

```rust
pub fn cosine_similarity(a: &Array1<f32>, b: &Array1<f32>) -> f32 {
    let dot = a.dot(b);
    let na  = a.dot(a).sqrt();
    let nb  = b.dot(b).sqrt();
    if na == 0.0 || nb == 0.0 { return 0.0; }
    dot / (na * nb)
}

pub fn score_cmp_desc(a: &(usize, f32), b: &(usize, f32)) -> Ordering {
    // NaN-aware descending comparator
    match (b.1.is_nan(), a.1.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal),
    }
}
```

`math.rs`. Zero-norm guard returns 0.0. NaN handling is explicit so the partial-sort never panics on partial_cmp returning None.

`CosineIndex::cosine_similarity(&a, &b)` is preserved as an associated fn that delegates to `math::cosine_similarity` for backwards compatibility with the integration test that calls `CosineIndex::cosine_similarity` directly.

### Persistent disk cache (Phase 5)

```rust
pub fn save_to_disk(&self)  // → Library/cosine_cache.bin
pub fn load_from_disk_if_fresh(&mut self, db_path: &Path)
```

`cosine/cache.rs`. Bincode-encoded `Vec<(PathBuf, Vec<f32>)>` written to `paths::cosine_cache_path()`. The `if_fresh` variant compares `cosine_cache.bin` mtime against the SQLite DB file's mtime — if the DB is newer (because the indexing pipeline wrote new embeddings since the last save), the cache is silently skipped and `populate_from_db` will rebuild it.

Save happens at the end of every indexing-pipeline run (`indexing.rs::run_pipeline_inner` `Phase::cosine_repopulate`). Load happens at the start (Step 0) so the user can run similarity queries within milliseconds of app launch on second-launch + later, before any of the rest of the pipeline finishes.

Stale cache invalidation comes "for free" via mtime — if a future code path writes to the DB without going through the pipeline (e.g., user-driven embed via a future "re-encode this image" command), the cache will look stale on next launch and rebuild.

### `Arc<Mutex<CosineIndex>>` shared across thread boundaries

```rust
pub struct CosineIndexState {
    pub index: Arc<Mutex<CosineIndex>>,    // shared with the indexing thread
    pub db_path: String,                    // legacy — kept because indexing thread needs it
}
```

`lib.rs:28-34`. The `Arc` lets the indexing thread (background) and the Tauri-managed state (foreground commands) hold clones of the same in-memory cache. Both point at the same vec; the indexing pipeline's repopulate immediately makes new embeddings available to the next foreground similarity query.

## Key Interfaces / Data Flow

### Inputs

| Source | Provides |
|--------|----------|
| `db.get_all_embeddings()` | (id, path, Vec<f32>) per non-null embedding |
| `Library/cosine_cache.bin` (via bincode) | Cached `Vec<(PathBuf, Vec<f32>)>` from previous session |
| `commands::similarity` and `commands::semantic` | `Array1<f32>` query embedding + top_n |
| `indexing.rs::run_pipeline_inner` Phase 0 + final | populate_from_db + save_to_disk calls |
| `commands::roots::*` | `cached_images.clear()` for cache invalidation |

### Outputs

| Destination | What |
|-------------|------|
| `commands::*` (similarity + semantic) | `Vec<(PathBuf, f32)>` — caller resolves paths to DB ids via `resolve_image_id_for_cosine_path` |
| `Library/cosine_cache.bin` | Bincode-encoded vec |
| Tracing spans `cosine.*` | Per-method timings for the perf report |

### State

- `cached_images: Vec<(PathBuf, Array1<f32>)>` — the index itself (~2 KB per 512-d embedding × N images = ~2 MB at 1000 images, ~20 MB at 10k)
- `scratch: Vec<(usize, f32)>` — reusable per-query buffer (~12 bytes per cached image)
- `Library/cosine_cache.bin` on disk — ~2 KB per embedding; loaded eagerly at indexing pipeline start

## Implemented Outputs / Artifacts

- 3 retrieval modes wired to 3 different commands.
- Persistent disk cache that survives across app launches.
- Reusable scratch buffer for allocation-free warm queries.
- 8 math unit tests + 5 cache disk-persistence tests + 6 partial-sort diagnostic tests.
- Tracing spans `cosine.populate_from_db`, `cosine.get_similar_images`, `cosine.get_similar_images_sorted`, `cosine.get_tiered_similar_images` for perf attribution.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Dimension mismatch panics on `ndarray` cosine math | A model swap that changes embedding dim | `a.dot(b)` requires matching shapes; panic propagates to the Mutex (poisoning it) and surfaces as `ApiError::Cosine("mutex poisoned")` for every subsequent query. |
| Stale cache invalidation only checks mtime, not content | Future code that writes embeddings without touching the DB | Wouldn't update DB mtime; cache would look fresh and serve stale embeddings. Today every embedding write goes through `db.update_image_embedding` which touches the DB. |
| Cosine Mutex serialises every similarity / semantic query | Concurrent UI actions | Two parallel queries serialise. Today's UI doesn't generate parallel queries; future "preload similar for hovered tile" would. |
| Persistent cache file can be corrupted | Disk failure mid-save | `load_from_disk_if_fresh` returns silently on bincode parse failure; `populate_from_db` rebuilds. Acceptable. |
| `Arc<Mutex<...>>` shared with indexing thread can poison from either side | Panic in either `populate_from_db` (indexing) or any retrieval method (commands) | Whole index unusable until restart. See `notes/mutex-poisoning.md`. |
| Memory grows linearly with library size | Very large libraries (100k+ images) | At 100k images × 2 KB per embedding = 200 MB RAM. Approaching the threshold where ANN structures (HNSW, etc.) start to make sense. See `enhancements/recommendations/02-hnsw-index-behind-trait.md`. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **HNSW or similar approximate nearest neighbour** behind a trait. Today's brute-force cosine over every cached image is O(N) per query; HNSW would be O(log N) at the cost of imperfect recall. Gated by reaching ~50k+ images in real use. Documented in `enhancements/recommendations/02-hnsw-index-behind-trait.md`.
- **Drop the `db_path: String` field on `CosineIndexState`** now that the cosine module no longer reads it. Currently it's still consumed by the indexing pipeline + commands::roots, but those could borrow from another source.
- **MMR / DPP retrieval modes** for diversity-aware re-ranking. Documented in `enhancements/recommendations/05-mmr-and-dpp-retrieval-modes.md`.
- **Per-query cache** for the same query embedding within a session (e.g., "more like this image" repeated within seconds). Today every call recomputes from scratch; a small LRU keyed on a hash of the query embedding would short-circuit the inner loop.

## Durable Notes / Discarded Approaches

- **`select_nth_unstable_by` + re-sort the trimmed top-K** is the right trade-off. A full sort is `O(N log N)`; partial select is `O(N)` for the partition + `O(K log K)` for the final sort. At N=10000, K=50 the partial path is 2.53× faster and the result is identical.
- **The diversity-pool sampler is intentional UX, not a bug.** `get_similar_images` does not return the strict top-N — it samples within the top 20% pool. The strict variant is `get_similar_images_sorted`. See `notes/random-shuffle-as-feature.md`.
- **The 7-tier sampler is load-bearing UX.** Tiers are deterministic (0-5%, 5-10%, ...); within each tier, 5 images are randomly selected; HashSet ensures no duplicates between tiers. The within-tier randomness keeps repeat views fresh; the tier structure keeps visual coherence.
- **`Arc<Mutex<CosineIndex>>` over channel-based ownership** because both the indexing thread (writes via `populate_from_db` + `save_to_disk`) and the foreground commands (reads via the 3 retrieval methods) need access. Channels would force serialisation; the Arc lets foreground reads happen concurrently with indexing-thread reads (the writes are brief).
- **Persistent cache uses bincode, not JSON or serde+msgpack.** Bincode is the smallest and fastest serde format for `Vec<(PathBuf, Vec<f32>)>`. The cache is binary by nature; human-readability isn't useful.
- **`cached_images.clear()` over selective pruning on root mutation.** A precise prune would require knowing which paths belong to which root; cheaper to just clear and let the next query repopulate.
- **`scratch` as `Vec<(usize, f32)>` not `Vec<(PathBuf, f32)>`.** Avoids cloning the PathBuf in the inner loop; the index→path resolution happens after the partial select on the surviving K results. Saves 30+ ns per cached image per query.

## Obsolete / No Longer Relevant

The pre-split single-file `cosine_similarity.rs` (860 lines) is gone (replaced by the shim). The old `populate_from_db(_db_path: &str)` signature is gone — the underscore-prefix that signalled "this parameter is awkward but not yet refactored" has been honoured. The previous N+1 per-image SELECT inside populate is gone (replaced by `get_all_embeddings`).
