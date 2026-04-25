# database

*Maturity: working*

## Scope / Purpose

Owns SQLite persistence for the entire backend: images table with embedded `f32` vectors stored as raw BLOBs, the tags catalogue, and the `images_tags` join table. Wraps a single `rusqlite::Connection` in a `Mutex` and exposes idempotent insert / query / update methods. Single source of truth for what files are indexed, what their thumbnails look like, what their CLIP embeddings are, and how they are tagged.

## Boundaries / Ownership

- **Owns:** the schema migration, the embedding-BLOB encoding/decoding, the `Mutex<Connection>` lifetime.
- **Does not own:** path normalisation (lives in `tauri-commands`), thumbnail generation (lives in `thumbnail-pipeline`), embedding generation (lives in `clip-image-encoder`).
- **Public API:** `ImageDatabase::new`, `initialize`, `add_image`, `get_images`, `get_all_images`, `get_images_with_thumbnails`, `get_images_without_embeddings`, `get_images_without_thumbnails`, `update_image_embedding`, `get_image_embedding`, `get_image_id_by_path`, `update_image_thumbnail`, `get_image_thumbnail_info`, `create_tag`, `delete_tag`, `get_tags`, `add_tag_to_image`, `remove_tag_from_image`, `default_database_path`.

## Current Implemented Reality

### Schema (3 tables)

```sql
CREATE TABLE images (
    id              INTEGER PRIMARY KEY,
    path            TEXT NOT NULL UNIQUE,
    embedding       BLOB,            -- raw little-endian f32 sequence; length = 512 * 4 bytes
    thumbnail_path  TEXT,            -- added at runtime via ALTER TABLE on existing DBs
    width           INTEGER,
    height          INTEGER
);

CREATE TABLE tags (
    id     INTEGER PRIMARY KEY,
    name   TEXT NOT NULL UNIQUE,
    color  TEXT NOT NULL             -- hex string e.g. "#3B82F6"
);

CREATE TABLE images_tags (
    image_id  INTEGER NOT NULL,
    tag_id    INTEGER NOT NULL,
    PRIMARY KEY (image_id, tag_id),
    FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id)   REFERENCES tags(id)   ON DELETE CASCADE
);
```

`db.rs:21-58` for create-if-not-exists DDL.

### Runtime migration (`migrate_add_thumbnail_columns`)

- Probes `PRAGMA table_info(images)` and inspects column names (`db.rs:62-81`).
- If `thumbnail_path` is missing, runs `ALTER TABLE images ADD COLUMN thumbnail_path TEXT`, then the same for `width` and `height`.
- This is the project's only migration path. There is no version table and no migration framework.

### Embedding BLOB encoding (the unsafe core)

```rust
let embedding_bytes: &[u8] = unsafe {
    std::slice::from_raw_parts(
        embedding.as_ptr() as *const u8,
        embedding.len() * std::mem::size_of::<f32>(),
    )
};
```

Source: `db.rs:264-274`. Symmetric decoding at `db.rs:309-316`. The decoder validates that the byte length is a multiple of `size_of::<f32>()` (4 bytes); a misaligned BLOB returns `FromSqlConversionFailure`.

The implementation:
- assumes native endianness (little-endian on every machine that has run this binary so far).
- assumes `*const f32` from a `*const u8` is properly aligned. SQLite BLOBs are typically aligned but the alignment is not guaranteed by the API — see Risks.
- avoids two allocations vs `bincode::serialize`/`Vec<u8>` shuffling.

### Locking model

- One `Mutex<Connection>` per `ImageDatabase` instance.
- Every method acquires `.lock().unwrap()` before SQL. 20 lock sites in `db.rs`; a panic with the lock held poisons the mutex for the rest of the session (see Risks).
- No `WAL` mode. No connection pool.
- The cosine module opens a **second** `Connection` to the same file via `ImageDatabase::new(_db_path)` (`cosine_similarity.rs:27`). Multiple connections to the same SQLite file is permitted by rusqlite, so this works in practice — but the cosine cache invariants live entirely outside SQLite, in process memory.

## Key Interfaces / Data Flow

| Read path | Used by | Returns |
|-----------|---------|---------|
| `get_images_with_thumbnails(filter_tag_ids, _filter_string)` | `tauri::get_images` (every grid load) | `Vec<ImageData>` shuffled by `rand::rng()` (`db.rs:496-499`); the `_filter_string` argument is **ignored** by SQL — the frontend passes `searchText` for cache-key reasons but the backend never looks at it. |
| `get_all_images()` | `cosine::populate_from_db`; `tauri::get_similar_images` flexible-match fallback | `Vec<ImageData>` sorted by id |
| `get_images_without_embeddings()` | `Encoder::encode_all_images_in_database` | `Vec<ImageData>` for rows where `embedding IS NULL` |
| `get_images_without_thumbnails()` | `ThumbnailGenerator::generate_all_missing_thumbnails` | `Vec<ImageData>` for rows where `thumbnail_path IS NULL OR thumbnail_path = ''` |
| `get_image_embedding(id)` | `CosineIndex::populate_from_db` (per row); `tauri::get_similar_images`/`get_tiered_similar_images` | `Vec<f32>` (deserialised) or `Err(QueryReturnedNoRows)` |
| `get_image_id_by_path(path)` | `tauri::*` for path → id mapping after cosine returns paths | `i64` or `Err(QueryReturnedNoRows)` |
| `get_image_thumbnail_info(id)` | `tauri::semantic_search` (enrich result with thumbnail/dims) | `Option<(thumbnail_path, width, height)>` |
| `get_tags()` | `tauri::get_tags` | `Vec<Tag>` ordered by id |

| Write path | Used by | Notes |
|------------|---------|-------|
| `add_image(path)` | `main::index_directory` (startup) | `INSERT OR IGNORE` — idempotent; safe to rerun |
| `update_image_embedding(id, Vec<f32>)` | `Encoder::encode_all_images_in_database` | Empty `Vec<f32>` is stored as empty BLOB explicitly (`db.rs:255-260`) — distinct from NULL. |
| `update_image_thumbnail(id, path, w, h)` | `ThumbnailGenerator::generate_all_missing_thumbnails` | Single `UPDATE` with all three columns at once |
| `create_tag(name, color)` | `tauri::create_tag` | Returns the new `Tag` with last-insert-rowid; `name` UNIQUE will error on duplicate |
| `add_tag_to_image(image_id, tag_id)` | `tauri::add_tag_to_image` | **`INSERT INTO`** — not `INSERT OR IGNORE`; second assignment of same pair errors at DB level. (`db.rs:121-127`) |
| `remove_tag_from_image(image_id, tag_id)` | `tauri::remove_tag_from_image` | Plain `DELETE` |
| `delete_tag(tag_id)` | **none** — implemented but **not registered** in `invoke_handler!` | Cascades to `images_tags` via `ON DELETE CASCADE` |

### Tag filter semantics (OR, not AND)

The grid filter SQL is:

```sql
WHERE EXISTS (
    SELECT 1 FROM images_tags it2
    WHERE it2.image_id = images.id
      AND it2.tag_id IN (?, ?, ...)
)
```

This returns images that have **any one** of the selected tags. The README and earlier memory-bank notes implied AND/OR support; the implementation is OR only.

## Implemented Outputs / Artifacts

- The on-disk `images.db` (top-level of the repo; `.gitignore`'d).
- The default DB path is the literal string `"../images.db"` (`db.rs:84-86`) — relative to `src-tauri/`'s working directory at runtime, which means the file lives at the repo root when `cargo tauri dev` is used.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `unsafe`-cast assumes alignment and native endianness | Moving the DB across architectures (e.g., x86_64 → ARM with different endianness — none today, but ARM64 macOS runs same-endian by accident not by guarantee). Misaligned BLOB allocation. | Embedding round-trip silently produces garbage `f32`s → cosine similarity becomes meaningless → semantic + similar search return junk. |
| Mutex poisoning is unrecoverable | Any `.unwrap()` panic on `lock()` when poisoned. Any panic *during* a held lock. | All subsequent DB calls fail with `Mutex poisoned`. The only recovery is restarting the app. |
| `add_tag_to_image` uses `INSERT` not `INSERT OR IGNORE` | A frontend or future caller assigning the same tag twice. | Returns DB error to the frontend. The current frontend pre-checks selection state, but a bug there would surface as a backend error string. |
| `delete_tag` is not exposed | A typo'd tag is created in the UI. | The tag lives forever; no UI path to remove it. The DB method works if called from Rust. |
| Tag filter is OR, not AND | A user expecting AND-semantics ("landscape AND sunset"). | They get OR-semantics with no UI signal. README is ambiguous on this. |
| No `WAL`, no connection pool | Concurrent commands try to write. | Today this cannot happen — Tauri's invoke_handler runs commands synchronously by default. If the app ever moves to spawn-task command handlers, contention becomes real. |
| Cosine module opens its own DB connection | `cosine_similarity.rs::populate_from_db(db_path)` constructs a new `ImageDatabase`. | Two connections coexist; if `db_path` is wrong (e.g., relative-path drift) the cosine cache silently populates from a different file. See `notes/path-and-state-coupling.md`. |
| Stale `filesystem.rs` test asserts `results.len() == 4` | Running `cargo test` against the committed `test_images/` (749 images). | Test fails — `cargo test` is currently red. |

## Partial / In Progress

None active. The schema has been stable since `0f013f7` (2025-12-13) when thumbnails were added.

## Planned / Missing / Likely Changes

- Replace ad-hoc `ALTER TABLE` migration with a versioned approach (a `schema_version` table + numbered migrations) before the next schema change. Today's pattern works for one delta; it does not scale.
- Add `INSERT OR IGNORE` to `add_tag_to_image` to harden against double-assignment.
- Register `delete_tag` as a Tauri command + add UI affordance.
- Decide AND vs OR semantics for tag filtering and either document or rewrite SQL with `GROUP BY image_id HAVING COUNT(DISTINCT tag_id) = ?` for AND.
- Optional: replace `unsafe` BLOB casts with `bytemuck::cast_slice` for a safe API equivalent.

## Durable Notes / Discarded Approaches

- **Embedding-as-BLOB was an explicit choice.** Alternatives (one row per dimension; serialised JSON; bincode) were not commented in source but the unsafe-cast approach is the only one in version control. The trade-off is: BLOB is space-efficient and round-trips f32 directly, but it is opaque to SQL — you cannot do nearest-neighbour search inside SQLite. That is fine because the cosine logic lives in `cosine_similarity.rs` operating on a Rust `Vec<(PathBuf, Array1<f32>)>` in memory.
- **The shuffle in `get_images_with_thumbnails` is intentional UX, not a bug.** Per commit body `36b33b66` (2025-12-17): "Images are now shuffled randomly instead of being sorted by ID. ... The frontend now invalidates the images query on modal close to ensure a new random order is fetched." Random order keeps the grid feeling fresh between visits. Sorted-by-id was the original behaviour and is preserved by `get_all_images`.
- **The `_filter_string` parameter is intentionally unused inside the SQL.** It is part of the cache key on the frontend (`useImages.ts:17`) so that React Query treats different search strings as different cache entries. The backend ignores it because tag filtering happens via `filter_tag_ids` and free-text search happens via the separate semantic-search command. This is wasted bandwidth (every keystroke creates a new cache entry containing identical data) — flagged in the LifeOS Gaps as a minor perf issue.
- **`get_images_with_thumbnails` does its own LEFT-JOIN aggregation in Rust** (`db.rs:457-494`) rather than using SQL `GROUP_CONCAT` because the result needs typed `Tag` rows, not flattened strings. The HashMap aggregation is O(rows × avg_tags_per_image) but is bounded by total tag assignments, not images squared.

## Obsolete / No Longer Relevant

- The original `get_images()` function is preserved alongside `get_images_with_thumbnails()` — only the latter is wired to a Tauri command. `get_images()` is still used internally by `get_all_images()` and the cosine populate loop. Both share the same LEFT-JOIN aggregation approach; the difference is only the thumbnail columns.
