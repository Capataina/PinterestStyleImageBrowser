# database

*Maturity: comprehensive*

## Scope / Purpose

Owns SQLite persistence for the entire backend: the multi-folder root catalogue, the images table with embedded `f32` vectors stored as raw BLOBs, free-text per-image notes, the orphan-detection flag, the tag catalogue, and the `images_tags` join table. Wraps a single `rusqlite::Connection` per `ImageDatabase` instance in a `Mutex` and exposes idempotent insert / query / update methods. **WAL journal mode** + `synchronous = NORMAL` + `foreign_keys = ON` are set at `initialize` time. Single source of truth for what files are indexed, what their thumbnails look like, what their CLIP embeddings are, what folders they came from, what notes the user wrote on them, and how they are tagged.

The module was previously a 1.6k-line `db.rs`; it is now split into focused submodules under `src-tauri/src/db/` with a `pub struct ImageDatabase` defined in `mod.rs` and `impl ImageDatabase { ... }` blocks distributed across the submodules. Public API surface is unchanged across the split — `db::ImageDatabase::add_image(...)`, `db.get_tags()`, etc., all continue to work because Rust merges inherent-impl blocks across files in the same crate.

## Boundaries / Ownership

- **Owns:** the schema (5 tables), the WAL+NORMAL+FK pragma block, the embedding-BLOB encoding/decoding via `bytemuck::cast_slice` (replaces 3 unsafe blocks), the per-table CRUD, the AND/OR tag filter SQL branch, the orphan-mark chunked UPDATE, the legacy migration helper, the pipeline-stats single-SELECT.
- **Does not own:** path normalisation (lives in `paths::strip_windows_extended_prefix`), thumbnail generation (lives in `thumbnail-pipeline`), embedding generation (lives in `clip-image-encoder`), root id resolution from cosine paths (lives in `commands::resolve_image_id_for_cosine_path`).
- **Public API surface:** `ImageDatabase::new`, `initialize`, `default_database_path`, `read_lock` (R2 — read-only secondary connection helper for foreground SELECTs), `checkpoint_passive` (R3 — manual WAL drain between encoder batches), `add_image`, `get_images`, `get_all_images`, `get_images_with_thumbnails`, `get_images_without_embeddings`, `get_images_without_thumbnails`, `get_image_id_by_path`, `get_paths_to_root_ids`, `get_pipeline_stats`, `update_image_embedding`, `get_image_embedding`, `get_all_embeddings`, `upsert_embedding`, `upsert_embeddings_batch` (R1 — BEGIN IMMEDIATE batch INSERT helper), `get_embedding`, `get_all_embeddings_for`, `get_images_without_embedding_for`, `count_embeddings_for`, `update_image_thumbnail`, `get_image_thumbnail_info`, `create_tag`, `delete_tag`, `get_tags`, `add_tag_to_image`, `remove_tag_from_image`, `list_roots`, `add_root`, `remove_root`, `set_root_enabled`, `migrate_legacy_scan_root`, `wipe_images_for_new_root`, `get_root_id_by_path`, `mark_orphaned`, `get_image_notes`, `set_image_notes`. Plus type alias `pub type ID = i64`.

## Current Implemented Reality

### Submodule layout

```
src-tauri/src/db/
├── mod.rs                — ImageDatabase struct, type ID, new(), initialize() (WAL/NORMAL/FK + CREATE TABLE
│                            + 3 idempotent migrations), default_database_path; tests::initialize_is_idempotent
├── schema_migrations.rs  — migrate_add_thumbnail_columns, migrate_add_multifolder_columns,
│                            migrate_add_notes_and_orphaned_columns (PRAGMA table_info gated)
├── images_query.rs       — aggregate_image_rows helper (audit extraction; was duplicated 4×),
│                            get_images / get_all_images / get_images_with_thumbnails (AND/OR branch),
│                            get_images_without_embeddings, get_images_without_thumbnails,
│                            get_paths_to_root_ids (single SELECT — replaced N+1 in the pipeline),
│                            get_image_id_by_path, get_pipeline_stats
├── embeddings.rs         — update_image_embedding + get_image_embedding via bytemuck::cast_slice,
│                            get_all_embeddings (single SELECT for cosine populate)
├── tags.rs               — create_tag, delete_tag, get_tags, add_tag_to_image (INSERT OR IGNORE),
│                            remove_tag_from_image
├── thumbnails.rs         — update_image_thumbnail, get_image_thumbnail_info
├── roots.rs              — list_roots, add_root, remove_root, set_root_enabled,
│                            migrate_legacy_scan_root, wipe_images_for_new_root, get_root_id_by_path
├── notes_orphans.rs      — add_image (multi-folder aware), get_image_notes, set_image_notes,
│                            mark_orphaned (chunked UPDATE for SQLite param limit)
└── test_helpers.rs       — fresh_db() helper used by every submodule's #[cfg(test)] block
```

The split was an audit Modularisation finding (composite hotspot score 0.98 — top in the repo). Public API was preserved exactly via Rust's automatic file-vs-directory module resolution and the `pub use` re-exports already in place — no caller changes anywhere.

### Schema (5 tables)

```sql
CREATE TABLE roots (
    id        INTEGER PRIMARY KEY,
    path      TEXT NOT NULL UNIQUE,
    enabled   INTEGER NOT NULL DEFAULT 1,
    added_at  INTEGER NOT NULL              -- unix epoch
);

CREATE TABLE images (
    id              INTEGER PRIMARY KEY,
    path            TEXT NOT NULL UNIQUE,
    embedding       BLOB,                    -- raw little-endian f32 sequence; length = 512 * 4 bytes typically
    thumbnail_path  TEXT,                    -- absolute path under <app_data_dir>/thumbnails/...
    width           INTEGER,
    height          INTEGER,
    root_id         INTEGER REFERENCES roots(id) ON DELETE CASCADE,  -- Phase 6 multi-folder
    notes           TEXT,                    -- Phase 11 free-text annotation
    orphaned        INTEGER NOT NULL DEFAULT 0   -- Phase 7 deleted-from-disk marker
);

CREATE TABLE tags (
    id     INTEGER PRIMARY KEY,
    name   TEXT NOT NULL UNIQUE,
    color  TEXT NOT NULL                     -- hex string e.g. "#3489eb"
);

CREATE TABLE images_tags (
    image_id  INTEGER NOT NULL,
    tag_id    INTEGER NOT NULL,
    PRIMARY KEY (image_id, tag_id),
    FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id)   REFERENCES tags(id)   ON DELETE CASCADE
);
```

Source: `db/mod.rs:90-143`. The `roots` table is created first because `images.root_id` references it.

### Pragmas at initialize

```rust
conn.pragma_update(None, "journal_mode", "WAL")?;
conn.pragma_update(None, "synchronous", "NORMAL")?;
conn.pragma_update(None, "busy_timeout", 5000)?;            // R3 — Tier 1 perf
conn.pragma_update(None, "wal_autocheckpoint", 0)?;         // R3 — manual via checkpoint_passive
conn.pragma_update(None, "journal_size_limit", 67_108_864)?; // R3 — 64 MiB cap
conn.execute("PRAGMA foreign_keys = ON;", [])?;
```

| PRAGMA | Why |
|--------|-----|
| `journal_mode = WAL` | The indexing pipeline opens its own `ImageDatabase` instance (a second SQLite connection to the same file). In default DELETE journal mode, the writer holds an exclusive lock for the duration of every write transaction, blocking all readers. WAL lets readers and the single writer coexist. SQLite's official recommendation for any multi-connection workload. |
| `synchronous = NORMAL` | Default `FULL` fsyncs after every commit — appropriate where torn writes corrupt structure, but unnecessary for this app where every commit is recoverable on next launch (tag mutations user can re-do, thumbnails / embeddings can be regenerated). NORMAL is SQLite's explicitly-recommended pairing with WAL when "lose at most the last commit on power loss" is acceptable. |
| `busy_timeout = 5000` (R3) | Default of 0 surfaces momentary lock contention (e.g. encoder batch commit while foreground IPC arrives) as `SQLITE_BUSY`. 5 s is generous enough that any real-world contention resolves transparently rather than reaching the user as an error. |
| `wal_autocheckpoint = 0` (R3) | SQLite's automatic checkpointer fires every 1000 dirty pages by default — and that cadence interleaves with encoder batch commits in a way that produces multi-second stalls (the trigger for the perf-1777212369 22 s freeze). We disable auto and call `checkpoint_passive()` ourselves between encoder batches so checkpoints land at known quiet points. |
| `journal_size_limit = 64 MiB` (R3) | Cap WAL file growth so it can't explode under bursty writes. The cap forces a truncate at the next quiet checkpoint, keeping disk usage bounded and reducing fsync cost at COMMIT. |
| `foreign_keys = ON` | SQLite defaults this OFF for backwards compatibility. Without it, `ON DELETE CASCADE` on `images.root_id → roots.id` is a no-op. The pragma is what made `remove_root` actually wipe the root's images. |

All pragmas are set in `initialize` after every connection open. WAL also persists across reopens (it's a property of the DB file). `pragma_update` is the rusqlite path that returns Result so we surface migration-time failures rather than ignoring them.

### R2 — read-only secondary connection

`ImageDatabase` holds two connections per real on-disk database:

| Field | Type | Use |
|-------|------|-----|
| `connection` | `Mutex<rusqlite::Connection>` | The writer. Every INSERT/UPDATE/DELETE goes through this mutex. Encoder pipeline holds it for the duration of each batch transaction; foreground IPC writes (tag mutations, root toggles) take it briefly. |
| `reader` | `OnceLock<Mutex<rusqlite::Connection>>` | A separate `SQLITE_OPEN_READ_ONLY` connection on the same file, opened lazily by `initialize()` after WAL mode is set. Used by foreground IPC SELECTs via `read_lock()`. `OnceLock` so `initialize` can populate through `&self`. |

For `:memory:` databases (tests), `reader` stays empty — `:memory:` is per-connection storage so a second connection sees a separate empty DB. `read_lock()` falls back to the writer in that case; tests don't have foreground/background contention to worry about anyway.

**Routing.** Foreground SELECTs go through `self.read_lock()`:
- `get_images_with_thumbnails` (the IPC freeze case)
- `get_images`, `get_image_id_by_path`, `get_pipeline_stats`
- `get_all_embeddings`, `get_all_embeddings_for`, `count_embeddings_for` (cosine cache populate, foreground)

The encoder writer keeps using `self.connection.lock()` directly. So a foreground `get_images` call no longer queues behind an in-flight encoder write batch — the two contend only at the SQLite WAL layer (which is non-blocking against active reads).

### R1 — encoder write batching via `upsert_embeddings_batch`

The encoder loops in `indexing.rs` write a chunk of (image_id, embedding) rows under one `BEGIN IMMEDIATE` transaction:

```rust
pub fn upsert_embeddings_batch(
    &self,
    encoder_id: &str,
    rows: &[(ID, Vec<f32>)],
    legacy_clip_too: bool,
) -> rusqlite::Result<()>
```

`BEGIN IMMEDIATE` rather than the default `DEFERRED` — `DEFERRED` upgrades to a write lock on the first INSERT, racing with any concurrent read; `IMMEDIATE` takes the write lock up-front. `legacy_clip_too` is now always `false` from both encoder loops as of R8 (the legacy `images.embedding` double-write was dropped).

Per the [PDQ benchmark](https://www.pdq.com/blog/improving-bulk-insert-speed-in-sqlite-a-comparison-of-transactions/), bulk inserts are 10-100× faster under one transaction than per-row autocommit. Combined with R2, the writer can run flat-out without affecting UI responsiveness.

### R3 — `checkpoint_passive` between encoder batches

```rust
pub fn checkpoint_passive(&self) -> rusqlite::Result<()> {
    if self.reader.get().is_none() { return Ok(()); }   // :memory: — no WAL file
    self.connection.lock().unwrap()
        .pragma_update(None, "wal_checkpoint", "PASSIVE")?;
    Ok(())
}
```

Called from both encoder loops between batches. PASSIVE mode does not block readers or writers — it processes whatever pages are clean and returns. Drives the WAL drain manually under `wal_autocheckpoint=0` so checkpoints land at predictable quiet points instead of mid-batch.

### Idempotent migrations

```rust
// Schema deltas (idempotent ALTER TABLE)
self.migrate_add_thumbnail_columns()?;       // adds thumbnail_path, width, height
self.migrate_add_multifolder_columns()?;     // adds root_id
self.migrate_add_notes_and_orphaned_columns()?;  // adds notes, orphaned

// ... CREATE TABLE for tags / images_tags / embeddings (with idx) ...

// One-shot embedding-pipeline invalidation when CLIP/DINOv2 pipeline
// changes invalidate prior embeddings. Runs AFTER the embeddings
// table is created (it issues DELETE against that table).
self.migrate_embedding_pipeline_version()?;
```

Each schema delta probes `PRAGMA table_info(images)` and runs `ALTER TABLE images ADD COLUMN ...` only if the column is missing. Idempotent — re-running on an up-to-date schema is a no-op.

The **embedding-pipeline version migration** is a different beast — it uses a separate `meta(key, value)` key-value table to record `embedding_pipeline_version`. When the stored version is less than `CURRENT_PIPELINE_VERSION` (currently `3` as of 2026-04-26), it deletes embeddings that were produced by the previous pipeline so the next indexing pass re-encodes everything cleanly. The version 3 bump wipes:

- `images.embedding` (legacy CLIP column — invalidated by the move from combined-graph multilingual to separate vision_model + OpenAI English text; R8 stops writing it on first encode under v3)
- `embeddings WHERE encoder_id = 'clip_vit_b_32'` (R6 + R7 changed the preprocessed RGB buffer fed into the encoder — fast_image_resize Lanczos3 + JPEG scaled IDCT produce subtly different bytes than the old image-rs CatmullRom + full IDCT path)
- `embeddings WHERE encoder_id = 'dinov2_small'` (legacy id from before the upgrade to dinov2_base; orphaned)
- `embeddings WHERE encoder_id = 'siglip2_base'` (same R6 + R7 reason — preprocessing buffer change invalidates SigLIP-2 embeddings too)
- `embeddings WHERE encoder_id = 'dinov2_base'` (same R6 + R7 reason)

`SigLIP-2` rows are not wiped because the SigLIP path wasn't producing embeddings before this version — there's nothing to invalidate.

Bump `CURRENT_PIPELINE_VERSION` whenever a future change invalidates existing embeddings (preprocessing geometry change, normalization stat change, output-extraction-method change, encoder-ID rename). The pattern keeps users from carrying stale embeddings into a new code path silently.

```sql
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- After wipe completes:
INSERT INTO meta VALUES ('embedding_pipeline_version', '2')
  ON CONFLICT(key) DO UPDATE SET value = excluded.value;
```

The `meta` table is a 6th table (in addition to roots, images, tags, images_tags, embeddings). Currently only one key. Future migrations could store schema_version, last_full_scan timestamp, etc. — kept minimal until a real need surfaces.

### Embedding BLOB encoding (now safe)

```rust
let embedding_bytes: &[u8] = bytemuck::cast_slice(&embedding);
self.connection.lock().unwrap().execute(
    "UPDATE images SET embedding = ?1 WHERE id = ?2",
    rusqlite::params![embedding_bytes, image_id],
)?;
```

`db/embeddings.rs:32-37`. `bytemuck::cast_slice` proves at compile time (via the `Pod` marker on `f32`) that the reinterpretation is safe — zero-copy view, same bytes hit the BLOB. Replaces 3 previous `unsafe { slice::from_raw_parts(...) }` blocks (audit Inconsistent Patterns finding `0bdb5f4`).

Symmetric decoding via `bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()`. The decoder also retains the runtime length-mod-4 check as belt-and-braces against malformed BLOBs:

```rust
if bytes.len() % f32_size != 0 {
    return Err(rusqlite::Error::FromSqlConversionFailure(...));
}
```

Empty embeddings (length 0) are stored explicitly as `&[]` (distinct from NULL) and round-trip as `Vec::new()`.

### `get_all_embeddings` — single SELECT for cosine

```rust
pub fn get_all_embeddings(&self) -> rusqlite::Result<Vec<(ID, String, Vec<f32>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, embedding FROM images
         WHERE embedding IS NOT NULL AND length(embedding) > 0",
    )?;
    // ... cast each row's bytes via bytemuck, skip rows whose length isn't a multiple of f32 size
}
```

`db/embeddings.rs:97-127`. Replaces the per-row `get_image_embedding(id)` call inside the cosine populate loop, which was N+1 (one query per image, ~30× slower for 1000+ image libraries). The cosine module's `populate_from_db(&ImageDatabase)` is the only caller.

### `get_paths_to_root_ids` — single SELECT for thumbnail routing

```rust
pub fn get_paths_to_root_ids(&self) -> rusqlite::Result<HashMap<String, Option<ID>>> {
    let mut stmt = conn.prepare("SELECT path, root_id FROM images")?;
    // collect into HashMap
}
```

`db/images_query.rs:342-349`. Replaces the indexing pipeline's previous `get_root_id_by_path(path)` per-image call, which held the DB Mutex 1500 times in rapid succession on a typical first run. Aligned with the existing `get_all_embeddings` shape — "fetch the whole table in one SELECT, the caller filters in memory" is the established pattern.

### Image grid SQL — root + orphan filter + AND/OR tag filter

```sql
-- Common WHERE clause for grid query:
WHERE images.orphaned = 0
  AND (
    images.root_id IS NULL
    OR images.root_id IN (SELECT id FROM roots WHERE enabled = 1)
  )
```

Plus optionally:

```sql
-- OR semantic (default; matches images with ANY of the selected tags):
AND EXISTS (
    SELECT 1 FROM images_tags it2
    WHERE it2.image_id = images.id
      AND it2.tag_id IN (?, ?, ...)
)

-- AND semantic (match_all_tags = true; matches images with EVERY selected tag):
AND images.id IN (
    SELECT it2.image_id
    FROM images_tags it2
    WHERE it2.tag_id IN (?, ?, ...)
    GROUP BY it2.image_id
    HAVING COUNT(DISTINCT it2.tag_id) = N
)
```

Source: `db/images_query.rs::get_images_with_thumbnails`. The frontend's `useUserPreferences.tagFilterMode` ("any" vs "all") is threaded through `useImages` → `fetchImages` → the `match_all_tags` parameter on the Tauri command. The query key includes `matchAllTags` so toggling re-fetches with fresh SQL semantics rather than serving cached OR results.

### `aggregate_image_rows` helper (audit extraction)

The "images LEFT JOIN images_tags LEFT JOIN tags" row-aggregation pattern was duplicated across 4 different fetch methods (each ~25 lines). The audit extracted a single helper:

```rust
fn aggregate_image_rows(rows: &mut rusqlite::Rows<'_>)
    -> rusqlite::Result<Vec<(ID, String, Vec<Tag>, Option<String>, Option<i64>, Option<i64>)>>
```

Each caller emits the standard column aliases (img_id, img_path, thumbnail_path, width, height, tag_id, tag_name, tag_color) — callers that don't have thumbnail data emit `NULL AS thumbnail_path`, `NULL AS width`, `NULL AS height` so the helper's `row.get("thumbnail_path")` resolves to `None`. The next change to tag-aggregation logic happens in one place; ditto the thumbnail-column shape.

### Stable grid order (no shuffle)

```rust
images.sort_by_key(|i| i.id);    // get_images_with_thumbnails
```

The previous "shuffle on every read" caused the visible "entire app refreshes" behaviour during indexing — every refetch (every ~2s while thumbnails were generating) reordered the grid, making tiles jump around. Sort modes are now controlled via the user's `sortMode` preference and applied frontend-side when needed (the frontend can apply a deterministic shuffle with a session seed if the user picks "shuffle"). Default sort mode is `"added"` — oldest first by id.

The frontend's modal-close-bumps-shuffleSeed pattern (`shuffleSeed` state in `[...slug].tsx`) means deliberate refresh actions trigger a new order; routine indexing-progress invalidations refetch with the SAME seed so the order stays stable through background updates.

### Pipeline stats — single SELECT

```rust
pub fn get_pipeline_stats(&self) -> rusqlite::Result<PipelineStats>
```

Returns counts of total / with_thumbnail / with_embedding / orphaned in one full-table scan via four `SUM(CASE WHEN ... THEN 1 ELSE 0 END)` aggregates. Lets the user see how much work the indexing pipeline has done without four separate Mutex acquisitions. Used by the upcoming pipeline-stats UI (planned).

### Orphan-detection chunked UPDATE

```rust
pub fn mark_orphaned(&self, root_id: ID, alive_paths: &[String]) -> rusqlite::Result<usize>
```

`db/notes_orphans.rs:27-80`. Two-pass approach without temp tables:
1. Reset every row in this root to `orphaned = 0` (so a renamed-back file reappears in the grid).
2. If `alive_paths` is empty, mark every row in this root orphaned (edge case: empty scan).
3. Otherwise, load all paths from the root, diff against the alive set in Rust (HashSet), and `UPDATE images SET orphaned = 1 WHERE id IN (...)` chunked at 500 ids per UPDATE to stay under SQLite's parameter limit on large libraries.

Returns the number of rows updated. Called by the indexing pipeline's scan phase, per root.

### Notes round-trip

```rust
pub fn get_image_notes(&self, image_id: ID) -> rusqlite::Result<Option<String>>
pub fn set_image_notes(&self, image_id: ID, notes: &str) -> rusqlite::Result<()>
```

`db/notes_orphans.rs:106-130`. `set_image_notes` trims whitespace and stores `None` (NULL) if the result is empty, otherwise the trimmed string. The user-facing semantic is "no annotation" for both empty and NULL.

### Locking model

- One `Mutex<rusqlite::Connection>` per `ImageDatabase` instance.
- Every method acquires `.lock().unwrap()` before SQL. ~30 lock sites across the submodules; a panic with the lock held poisons the mutex for the rest of the session (see `notes/mutex-poisoning.md`).
- The indexing pipeline opens a **second** `ImageDatabase::new(db_path)` connection on its background thread. Multiple connections to the same SQLite file under WAL is the supported pattern — readers don't block the single writer.

## Key Interfaces / Data Flow

| Read path | Used by | Returns |
|-----------|---------|---------|
| `get_images_with_thumbnails(filter_tag_ids, _filter_string, match_all_tags)` | `commands::images::get_images` (every grid load) | `Vec<ImageData>` sorted by id; the `_filter_string` is preserved in the cache key but the SQL ignores it |
| `get_all_images()` | `commands::resolve_image_id_for_cosine_path` flexible-match fallback | `Vec<ImageData>` sorted by id |
| `get_images_without_embeddings()` | indexing pipeline encode phase | Rows where `embedding IS NULL` |
| `get_images_without_thumbnails()` | indexing pipeline thumbnail phase | Rows where `thumbnail_path IS NULL OR ''` |
| `get_image_embedding(id)` | `commands::similarity` (per-image query embedding lookup) | `Vec<f32>` or `Err(QueryReturnedNoRows)` |
| `get_all_embeddings()` | `cosine.populate_from_db(&db)` | `Vec<(ID, path, Vec<f32>)>` non-null only — single SELECT |
| `get_image_id_by_path(path)` | `commands::resolve_image_id_for_cosine_path` strategies 1 + 2 | `i64` or `Err(QueryReturnedNoRows)` |
| `get_paths_to_root_ids()` | indexing pipeline thumbnail routing | `HashMap<path, Option<root_id>>` — single SELECT |
| `get_image_thumbnail_info(id)` | `commands::semantic` enrich result | `Option<(thumbnail_path, w, h)>` |
| `get_pipeline_stats()` | (planned UI; unit tests verify shape) | `PipelineStats { total, with_thumbnail, with_embedding, orphaned }` |
| `get_tags()` | `commands::tags::get_tags`, every grid filter UI | `Vec<Tag>` ordered by id |
| `list_roots()` | `commands::roots::list_roots`, indexing pipeline, watcher start | `Vec<Root>` ordered by added_at |
| `get_image_notes(id)` | `commands::notes::get_image_notes` | `Option<String>` |

| Write path | Used by | Notes |
|------------|---------|-------|
| `add_image(path, root_id)` | indexing pipeline scan phase | `INSERT OR IGNORE` on path UNIQUE — idempotent. `root_id: Option<ID>` because legacy un-migrated rows are NULL. |
| `update_image_embedding(id, Vec<f32>)` | indexing pipeline encode phase | bytemuck::cast_slice; empty Vec stored as empty BLOB explicitly |
| `update_image_thumbnail(id, &Path, w, h)` | indexing pipeline thumbnail phase | Single UPDATE with all 3 columns at once |
| `mark_orphaned(root_id, alive_paths)` | indexing pipeline scan phase, per root | Reset-then-mark via HashSet diff + chunked UPDATE |
| `add_root(path)` | `commands::roots::add_root`, `set_scan_root` | Returns the populated `Root`; UNIQUE constraint surfaces as `Err` (mapped to `ApiError::Db`) |
| `remove_root(id)` | `commands::roots::remove_root` | CASCADE wipes images via the FK |
| `set_root_enabled(id, bool)` | `commands::roots::set_root_enabled` | Grid filter query reads the column directly — instant toggle, no re-index |
| `wipe_images_for_new_root()` | `commands::roots::set_scan_root` | Clears NULL-root_id legacy rows when replacing all roots |
| `migrate_legacy_scan_root(path)` | `lib.rs::run::setup` (one-shot) | Idempotent; backfills NULL-root_id rows whose path starts with the legacy root |
| `create_tag(name, color)` | `commands::tags::create_tag` | Returns the new `Tag` with last-insert-rowid |
| `delete_tag(id)` | `commands::tags::delete_tag` (NOW WIRED — was orphaned pre-Phase-6) | CASCADE-deletes from `images_tags` |
| `add_tag_to_image(image_id, tag_id)` | `commands::tags::add_tag_to_image` | `INSERT OR IGNORE` (Phase 6 hardening — was plain INSERT before, errored on duplicate) |
| `remove_tag_from_image(image_id, tag_id)` | `commands::tags::remove_tag_from_image` | Plain DELETE |
| `set_image_notes(id, &str)` | `commands::notes::set_image_notes` | Empty string clears (stores NULL) |

## Implemented Outputs / Artifacts

- The on-disk `<app_data_dir>/images.db` (+ `images.db-wal` + `images.db-shm` files when WAL is active). All gitignored.
- The `default_database_path()` helper returns the platform-correct path via `paths::database_path()` — on macOS `~/Library/Application Support/com.ataca.image-browser/images.db`. Same path in dev and release as of 2026-04-26; override via `IMAGE_BROWSER_DATA_DIR` env var.
- 50+ unit tests across the submodule `tests` blocks: schema idempotency, AND/OR tag semantics, multi-folder filter, NULL-root_id legacy rows, orphan detection (incl. 1200-id chunking stress test), notes round-trip, embedding BLOB round-trip (incl. large + empty), pipeline stats correctness across each stage.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Endianness assumption (little-endian) | Moving the DB across architectures with different endianness (none today, but ARM64 macOS happens to match little-endian by accident not by guarantee) | Embedding round-trip silently produces garbage f32s → cosine similarity becomes meaningless. Mitigated by bytemuck's compile-time alignment proof but not endianness guard. |
| Mutex poisoning is unrecoverable | A panic with the connection mutex held | All subsequent DB calls fail with `Mutex poisoned` (surfaced as `ApiError::Db("...")` via the rusqlite path or as the foreground process getting stuck). The only recovery is restarting. See `notes/mutex-poisoning.md`. |
| `add_root` UNIQUE error surfaces as generic `ApiError::Db` | User adds the same folder twice via add_root | Frontend gets a typed-but-generic message. Could be sharpened to `ApiError::BadInput("already added")`. |
| No `WAL` checkpointing strategy | Long sessions with many writes | The `-wal` file can grow unboundedly. SQLite auto-checkpoints at 1000 pages by default but the user might see a large `-wal` file briefly. Cosmetic. |
| `_filter_string` parameter unused in SQL | Frontend passes searchText for cache-key purposes; backend doesn't filter on it | Wasted bandwidth (every keystroke creates a new cache entry containing identical data). Documented; minor perf issue. |
| No version table for schema migrations | A future fourth migration that needs ordering / backfill / data refactor | Today's `if column missing then ALTER TABLE` works for additive changes; non-additive changes need a real migration framework. |
| `mark_orphaned` chunks at 500 ids per UPDATE | Libraries with hundreds of newly-orphaned images | Multiple sequential UPDATEs run inside the indexing thread. Bounded but not parallel. |
| Path comparison in `get_image_id_by_path` is exact string match | Trailing slash, case differences (Windows), Unicode normalisation differences | Falls through to the flexible-match fallback in `commands::resolve_image_id_for_cosine_path` strategy 3. The fallback handles it but has its own cost. See `notes/path-and-state-coupling.md`. |
| Stale unit test — historical | Was: `test_scan_directory_finds_all_images` asserted len==4 against test_images/ | Resolved: that test is gone (commit `12d9b07` removed the paid dataset and the broken hardcoded-path tests). |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Versioned migration framework** before the next non-additive schema change. Add a `schema_version` table + numbered up/down migrations. Today's pattern works for one delta; it does not scale.
- **Sharper error types for known constraint violations** — `add_root`'s UNIQUE error should surface as `ApiError::BadInput("already added")` rather than generic `ApiError::Db`.
- **Endianness guard for embedding BLOBs** — write a magic byte sequence as a header so loading a wrong-endian BLOB errors loudly instead of producing garbage.
- **WAL checkpoint hint in shutdown** — call `wal_checkpoint(TRUNCATE)` on app exit so the `-wal` file shrinks. Cosmetic.
- **Pipeline stats UI** — `get_pipeline_stats` is implemented and tested but not yet surfaced. Plan: add to the Settings drawer or status pill (`plans/pipeline-parallelism-and-stats-ui.md`).

## Durable Notes / Discarded Approaches

- **Embedding-as-BLOB was an explicit choice.** Alternatives (one row per dimension; serialised JSON; bincode) were considered but BLOB is space-efficient and round-trips f32 directly. The trade-off is opacity to SQL — you can't do nearest-neighbour search inside SQLite — but that's fine because cosine logic lives in the cosine module operating on a Rust `Vec<(PathBuf, Array1<f32>)>` in memory.
- **`bytemuck::cast_slice` over `unsafe slice::from_raw_parts`** because the `Pod` marker on `f32` proves at compile time that the reinterpretation is safe. Same zero-copy view, same bytes; no `unsafe` block. Audit Inconsistent Patterns finding `0bdb5f4`.
- **The `_filter_string` parameter is intentionally unused inside the SQL.** It's part of the cache key on the frontend (`useImages.ts`) so React Query treats different search strings as different cache entries. The backend ignores it because tag filtering happens via `filter_tag_ids` and free-text search happens via the separate semantic-search command.
- **`get_images_with_thumbnails` does its own LEFT-JOIN aggregation in Rust** rather than using SQL `GROUP_CONCAT` because the result needs typed `Tag` rows, not flattened strings. The `aggregate_image_rows` helper centralises this so the next change happens in one place.
- **Stable sort by id (oldest first), not random shuffle.** The previous shuffle-on-every-read caused the visible "entire app refreshes" behaviour during indexing; every refetch reordered the grid. Sort modes now live in frontend `useUserPreferences`; the backend returns deterministic order. See `notes/random-shuffle-as-feature.md` for the historical context.
- **AND vs OR tag filter is opt-in.** Default is OR (`Any`) which preserves the previous behaviour. Users who want AND flip the toggle in Settings → Search → Tag filter. The query key includes `matchAllTags` so toggling re-fetches with fresh SQL semantics.
- **WAL was the explicit fix for foreground/background DB contention.** Pre-WAL, the indexing pipeline's writes blocked every UI read; the grid would freeze for seconds at a time during encode. WAL eliminates the blocking; foreground reads stay responsive while background writes proceed.
- **`PRAGMA foreign_keys = ON` is required for CASCADE to work.** SQLite defaults this OFF. Without it, removing a root would leave its image rows orphaned forever.

## Obsolete / No Longer Relevant

The pre-split single-file `db.rs` (1597 lines, audit's top hotspot) is gone. The previous `get_images()` "no thumbnail data" function still exists alongside `get_images_with_thumbnails` — the former is used by `get_all_images()` and tests; the latter is the one the grid command uses. Both share the `aggregate_image_rows` helper.
