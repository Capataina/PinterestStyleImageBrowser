# cosine-similarity

*Maturity: working*

## Scope / Purpose

In-memory similarity index over the database's CLIP embeddings. Provides three retrieval modes: random-sampled top-N (legacy), strictly-sorted top-N (semantic search), and Pinterest-style tiered sampling (visual similarity). Constructed empty at app startup and populated on the first query that needs it. Owns no persistence — the canonical embeddings live in SQLite as BLOBs.

## Boundaries / Ownership

- **Owns:** in-memory `Vec<(PathBuf, Array1<f32>)>` cache, the cosine math, the three sampling strategies, NaN handling.
- **Does not own:** persistence (re-builds from DB on first use), embedding generation, exclude-path normalisation (caller passes a normalised `&PathBuf`).
- **Public API:** `CosineIndex::new()`, `add_image(path, embedding)`, `populate_from_db(&str)`, `cosine_similarity(a, b)` (associated fn), `get_similar_images(embedding, top_n, exclude_path)`, `get_similar_images_sorted(embedding, top_n, exclude_path)`, `get_tiered_similar_images(embedding, exclude_path)`.

## Current Implemented Reality

### Population (lazy, once per session)

```rust
pub fn populate_from_db(&mut self, _db_path: &str) {
    let db = db::ImageDatabase::new(_db_path).expect("failed to init db");
    let images = db.get_all_images().expect("failed to get all images");
    for image in images {
        match db.get_image_embedding(image.id) {
            Ok(embedding) if !embedding.is_empty() => {
                self.add_image(PathBuf::from(image.path), Array1::from_vec(embedding));
            }
            Ok(_) => {} // empty embedding — skip silently
            Err(_)  => {} // no embedding row — skip silently
        }
    }
}
```

Source: `cosine_similarity.rs:22-61`. Three counters are tracked (`added_count`, `skipped_no_embedding`, `skipped_empty`) and logged. **Note:** this constructs a *new* `ImageDatabase` from the stored `db_path`, rather than borrowing the existing connection — a documented coupling smell. See "Surprising Connection" below.

### Cosine math

```rust
fn cosine_similarity(a: &Array1<f32>, b: &Array1<f32>) -> f32 {
    let dot = a.dot(b);
    let na  = a.dot(a).sqrt();
    let nb  = b.dot(b).sqrt();
    if na == 0.0 || nb == 0.0 { return 0.0; }
    dot / (na * nb)
}
```

Source: `cosine_similarity.rs:64-75`. Zero-norm guard returns 0.0. NaN handling in sorting is explicit (`is_nan` cases enumerated in the comparator at `cosine_similarity.rs:121-127, 211-216, 282-287`).

### Three retrieval modes

| Mode | Method | Returns |
|------|--------|---------|
| Diversity-sampled | `get_similar_images(emb, top_n, exclude_path)` | Sort by cosine desc, take top max(top_n, 20% of pool), randomly pick `top_n` from that pool |
| Strict-sorted | `get_similar_images_sorted(emb, top_n, exclude_path)` | Sort by cosine desc, take exactly the first `top_n` |
| 7-tier | `get_tiered_similar_images(emb, exclude_path)` | 5 random per tier × 7 tiers (0-5%, 5-10%, 10-15%, 15-20%, 20-30%, 30-40%, 40-50%); deduplicated via `HashSet<usize>` |

The tiered method deserves the most attention: it is the most product-thoughtful piece of code in the backend. The tier definitions live as a literal array (`cosine_similarity.rs:295-303`):

```rust
let tiers = [
    (0.00, 0.05, 5), (0.05, 0.10, 5), (0.10, 0.15, 5),
    (0.15, 0.20, 5), (0.20, 0.30, 5), (0.30, 0.40, 5), (0.40, 0.50, 5),
];
```

This produces up to 35 results from a stratified sample across the top 50% — it gives the user the *very* similar images (top 5%), images that are obviously related (5-20%), and images that are vaguely related (20-50%) all mixed into one Pinterest-style result set. A pure top-K retrieval would feel monotonous; the tiered approach keeps the result feed visually varied.

The within-tier randomness uses `rand::rng().choose_multiple(...)` and the 50% cap is intentional: anything below that is effectively unrelated by cosine. The `HashSet<usize>` of used indices guarantees no duplicates between tiers.

### Sort comparator

NaN handling is enumerated rather than relying on `unwrap_or`:

```rust
match (b.1.is_nan(), a.1.is_nan()) {
    (true, true)  => Equal,
    (true, false) => Greater,   // b is NaN — a comes first (NaN sorts last)
    (false, true) => Less,      // a is NaN — b comes first
    (false, false) => b.1.partial_cmp(&a.1).unwrap(),
}
```

Repeated identically at `cosine_similarity.rs:121, 211, 282`. This is consistent enough that it could be a free function — a small refactor opportunity.

## Key Interfaces / Data Flow

```text
tauri::get_similar_images(image_id, top_n)            ── randomised diversity pool
tauri::get_tiered_similar_images(image_id)            ── 7-tier sampler
tauri::semantic_search(query, top_n)                  ── strict sorted (semantic ranking matters)
    │
    ▼
cosine_state.lock() → if empty, populate_from_db
    │
    ▼
db.get_image_embedding(id) for the query image (similar paths)
text_encoder.encode(query) for semantic
    │
    ▼
CosineIndex::{get_similar_images,get_similar_images_sorted,get_tiered_similar_images}
    │
    ▼
Vec<(PathBuf, f32)> (path-only — caller resolves to image ids via path-normalisation fallback)
```

## Implemented Outputs / Artifacts

- The `cached_images: Vec<(PathBuf, Array1<f32>)>` lives in `CosineIndexState.index` (a `Mutex<CosineIndex>`).
- No on-disk artefacts.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `populate_from_db` opens its own DB connection | Every first similarity/semantic query | See Surprising Connection below. Mostly fine today; fragile under DB-path drift. |
| N+1 queries on populate | First similarity/semantic query at startup | One `get_image_embedding` per row. For 1000 images = 1000 sequential SELECTs. A `db.get_all_embeddings()` returning `(id, blob)` pairs in one query would be ~30× faster. |
| Cache is never invalidated | If runtime rescan ever ships and adds new embeddings | New images do not enter `cached_images` until the process restarts. Not a problem today because there is no runtime add — but the precondition is fragile. |
| Mutex held during whole sort | Concurrent semantic + tiered queries | The two operations serialise through `cosine_state.lock()`. Fine for current usage; would block if queries became frequent. |
| `panic!` on `unwrap()` in comparators | A `partial_cmp` returning None — only possible if both sides are NaN, which the comparator already covers — so this is effectively dead, but `.unwrap()` lives in code where a future refactor could break the invariant. |
| Float-only similarity, no half-precision | A future move to fp16 embeddings | Would require a parallel BLOB encoding path and conversion to/from f32 for cosine math. Not on roadmap. |

## Surprising Connection

The `populate_from_db(db_path: &str)` signature opens a **second `ImageDatabase` connection** to the same SQLite file rather than borrowing the existing `&ImageDatabase` (`cosine_similarity.rs:27`). The reason is structural: `CosineIndexState` cannot hold a reference to the existing `ImageDatabase` because the existing `ImageDatabase` is itself in Tauri-managed state with no easy way to make it share-by-reference with another piece of Tauri-managed state.

Consequences:
- Two `Mutex<Connection>`s exist over the same SQLite file. Rusqlite handles this; SQLite's default journal mode handles it.
- The `db_path` string lives twice: once in `CosineIndexState.db_path`, once in `main.rs::default_database_path()`.
- A test for cosine populate cannot use an in-memory DB easily because `ImageDatabase::new(":memory:")` produces a different connection than the in-memory one initially populated.

A cleaner design would be `populate_from_db(&mut self, db: &ImageDatabase)` where the caller passes the existing managed instance. This would also allow the N+1 fix (`db.get_all_embeddings()`).

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Refactor `populate_from_db` to take `&ImageDatabase`.
- Add `db.get_all_embeddings()` and call it once instead of N times.
- Add `cosine_index.add_image(path, embedding)` calls to any embed-then-store flow once runtime rescan ships, to keep the cache fresh.
- Optional: cache `cached_images` to disk via `bincode` and lazy-load at startup, avoiding the populate cost. Today's startup populate is sub-second; at 100k images it would be a real wait.
- Optional: replace brute-force cosine over all rows with HNSW (`instant-distance` crate) once row counts pass ~10⁵.

## Durable Notes / Discarded Approaches

- **The 7-tier sampling algorithm is intentional product design and should be preserved across refactors.** A future "simplification" pass might want to collapse it to a top-K. Don't. The tier structure, within-tier randomness, 50% cap, and `HashSet<usize>` no-reuse constraint are all load-bearing UX choices. The verbose comments at `cosine_similarity.rs:248-253` are intentional documentation, not noise. Per commit `3fff0dd` (2025-12-13): "Pinterest-style tiered similar image search."
- **The two methods `get_similar_images` and `get_similar_images_sorted` look near-duplicate but are not.** The diversity-pool sampler exists for "show me images sort of like this one but not all the same" — useful when the top result set is full of near-duplicates. The strictly-sorted variant exists for semantic search where ranking accuracy matters more than visual diversity. The split was intentional in commit `930f1fc` (2025-12-17): "Added get_similar_images_sorted to CosineIndex for accurate, sorted semantic search results."
- **Skipping rows without embeddings rather than panicking** was added in commit `8a7252e6` (2025-12-11). Earlier code panicked on missing embeddings — fragile in any state where the DB had a row but encoding had not yet completed.

## Obsolete / No Longer Relevant

None — the three retrieval modes are all currently in use.
