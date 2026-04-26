# multi-folder-roots

*Maturity: comprehensive*

## Scope / Purpose

The "user can configure several folders, toggle them on and off independently, and remove them without losing other folders' data" subsystem. Owns the `roots` SQLite table, the per-root foreign-key relationship from `images.root_id` (with `ON DELETE CASCADE`), the per-root thumbnail directory layout, the legacy single-folder migration path, and the four CRUD Tauri commands that drive the Settings drawer's Folders section.

This is what made the Phase 6 transition from "one folder, replace to switch" to "any number of folders, toggle individually" possible.

## Boundaries / Ownership

- **Owns:** the `roots` table schema + CRUD, `images.root_id` FK + cascade behaviour, the `set_scan_root` "replace all roots" semantic, `add_root` / `remove_root` / `set_root_enabled` granular semantics, `migrate_legacy_scan_root`, `wipe_images_for_new_root`, `paths::thumbnails_dir_for_root(root_id)`.
- **Does not own:** the indexing pipeline that gets re-spawned after every root mutation (delegates to `indexing::try_spawn_pipeline`), the cosine cache invalidation (delegates to `commands::roots` clearing `cached_images`), the watcher reconfiguration (today: gap — see `systems/watcher.md`).
- **Public API:** `db.list_roots()`, `db.add_root(path)`, `db.remove_root(id)`, `db.set_root_enabled(id, enabled)`, `db.migrate_legacy_scan_root(path)`, `db.wipe_images_for_new_root()`, `db.get_root_id_by_path(path)`. Tauri commands: `list_roots`, `add_root`, `remove_root`, `set_root_enabled`, `set_scan_root`, `get_scan_root`.

## Current Implemented Reality

### Schema

```sql
CREATE TABLE roots (
    id        INTEGER PRIMARY KEY,
    path      TEXT NOT NULL UNIQUE,
    enabled   INTEGER NOT NULL DEFAULT 1,
    added_at  INTEGER NOT NULL    -- unix epoch seconds
);

-- images.root_id added via Phase 6 migration:
ALTER TABLE images
ADD COLUMN root_id INTEGER REFERENCES roots(id) ON DELETE CASCADE;
```

`db/mod.rs:90-98` for the create. `db/schema_migrations.rs` for the idempotent ALTER TABLE.

`PRAGMA foreign_keys = ON` is set in `initialize` — without it, `ON DELETE CASCADE` would silently no-op. This is the explicit fix that made root removal actually wipe its images.

### Two distinct UX semantics

The system exposes two ways to change which folders are indexed:

| Command | Semantic | When used |
|---------|----------|-----------|
| `set_scan_root(path)` | **Replace all roots with one new one.** Removes every existing root (CASCADE wipes their images), wipes orphan rows from older NULL-root_id imports, adds the new root, clears the cosine cache, spawns the indexing pipeline. | No frontend caller after the 2026-04-26 top-bar rename ("Choose folder" → "Add folder"). Tauri command + tests retained for the legacy mental model in case a "Reset library" UX is reintroduced. |
| `add_root(path)` / `remove_root(id)` / `set_root_enabled(id, enabled)` | **Granular per-root mutation.** `add_root` inserts a new row + spawns reindex (existing roots untouched). `remove_root` CASCADE-deletes the root's images + per-root thumbnail subfolder. `set_root_enabled` toggles the `enabled` column — no reindex needed because the grid query filters by enabled status. | Both the top-bar "Add folder" pill and the Settings drawer Folders section call `add_root` via the `useAddRoot` mutation, so the new row immediately appears in the Folders list (the mutation invalidates `["roots"]`). The drawer also exposes per-row toggle + remove. |

Both paths preserve the `tags` and `images_tags` tables — tag catalog persists across root reorganisation.

### Grid filtering

Every `get_images_with_thumbnails` SQL gates on:

```sql
WHERE images.orphaned = 0
  AND (
    images.root_id IS NULL
    OR images.root_id IN (SELECT id FROM roots WHERE enabled = 1)
  )
```

`db/images_query.rs:236-242`. `root_id IS NULL` rows are legacy un-migrated images (from before Phase 6) and are kept in the grid so existing libraries don't disappear after upgrade.

Disabling a root is instant — the row stays, the SQL filter excludes its images, re-enabling just shows them again. No re-encode, no re-thumbnail, no DB change beyond the `enabled` column.

### Per-root thumbnail layout

Pre-Phase-9 layout was flat:
```
Library/thumbnails/thumb_42.jpg
Library/thumbnails/thumb_43.jpg
...
```

Post-Phase-9 layout is per-root:
```
Library/thumbnails/root_1/thumb_42.jpg
Library/thumbnails/root_2/thumb_99.jpg
```

The reorg means `remove_root` can `rm -rf` the root's subfolder in one filesystem call, instead of per-row file deletion. Old `root_id = NULL` images still write to the flat layout via `paths::thumbnails_dir()` directly.

The `ThumbnailGenerator::generate_thumbnail(path, image_id, root_id: Option<i64>)` API takes the root_id; `None` falls back to the flat layout.

### Legacy migration

```rust
// lib.rs::run::setup, runs once at app launch
let user_settings = settings::Settings::load();
if let Some(legacy_path) = user_settings.scan_root.clone() {
    if let Ok(temp_db) = ImageDatabase::new(&db_path) {
        let _ = temp_db.initialize();
        match temp_db.migrate_legacy_scan_root(legacy_path.to_string_lossy().into_owned()) {
            Ok(Some(root)) => {
                info!("migrated legacy scan_root -> roots[{}] ({})", root.id, root.path);
                let mut s = user_settings.clone();
                s.scan_root = None;
                let _ = s.save();   // clear so we don't re-migrate
            }
            Ok(None) => {} // already migrated
            Err(e) => warn!("legacy migration failed: {e}"),
        }
    }
}
```

`migrate_legacy_scan_root` is idempotent: if a row already exists for that path, it returns `Ok(None)` and nothing happens. The post-success `scan_root = None` clear means subsequent launches don't re-attempt the migration. Backfills any `images.root_id = NULL` rows whose path starts with the legacy path so they get associated with the new root.

### Cosine cache invalidation

Every root-mutating command clears `CosineIndexState.index.cached_images` directly:

```rust
if let Ok(mut idx) = cosine_state.index.lock() {
    idx.cached_images.clear();
}
```

This forces the next similarity / semantic call to repopulate from the (now post-mutation) DB. The persistent `cosine_cache.bin` on disk also becomes stale and is overwritten by the next pipeline run's `save_to_disk`.

## Key Interfaces / Data Flow

### `set_scan_root(path)` lifecycle

```
Frontend (FoldersSection or empty-state):
  invoke("set_scan_root", { path })
        └─── Tauri IPC ───
commands::roots::set_scan_root:
  • Validate path is a directory; else ApiError::BadInput
  • db.list_roots() → for each: db.remove_root(r.id)  ← CASCADE wipes images
  • db.wipe_images_for_new_root()  ← clears any NULL-root_id legacy rows
  • db.add_root(path)
  • Clear cosine_state.index.cached_images
  • try_spawn_pipeline(...)   ← background indexing starts
  Returns Ok(())
        ─── Tauri IPC ───
Frontend:
  Settings drawer or empty-state UI updates (useRoots query refetches)
  IndexingStatusPill starts showing progress events
```

### `add_root(path)` lifecycle

```
Frontend:
  invoke("add_root", { path })
        ─── Tauri IPC ───
commands::roots::add_root:
  • Validate is_dir; else ApiError::BadInput
  • db.add_root(path) → returns Root (including new id)
  • try_spawn_pipeline(...)  ← incremental rescan picks up the new root
  Returns Ok(root)
        ─── Tauri IPC ───
Frontend:
  useAddRoot mutation onSuccess invalidates ["roots"] query
  IndexingStatusPill renders progress
```

### `remove_root(id)` lifecycle

```
Frontend (FoldersSection × button):
  invoke("remove_root", { id })
        ─── Tauri IPC ───
commands::roots::remove_root:
  • db.remove_root(id) → CASCADE wipes every images.row whose root_id = id
  • paths::thumbnails_dir_for_root(id) → if exists, rm -rf (best-effort, log warn on fail)
  • Clear cosine_state.index.cached_images
  Returns Ok(())
        ─── Tauri IPC ───
Frontend:
  useRemoveRoot mutation onSuccess invalidates ["roots"] AND ["images"] queries
```

### `set_root_enabled(id, enabled)` lifecycle

```
Frontend (FoldersSection toggle):
  invoke("set_root_enabled", { id, enabled })
        ─── Tauri IPC ───
commands::roots::set_root_enabled:
  • db.set_root_enabled(id, enabled)
  • Clear cosine_state.index.cached_images  ← so similarity reflects active set
  Returns Ok(())
        ─── Tauri IPC ───
Frontend:
  useSetRootEnabled mutation onSuccess invalidates ["roots"] AND ["images"]
  Grid re-renders without the disabled root's images (or with them, on enable)
```

## Implemented Outputs / Artifacts

- 4 root-management Tauri commands + 1 legacy `set_scan_root` that wraps them + 1 `get_scan_root` for backwards-compat with the empty-state UI
- 1 system table (`roots`) + 1 column on `images` (`root_id`)
- 1 thumbnail subdirectory per root, `Library/thumbnails/root_<id>/`
- The Settings drawer's Folders section
- `useRoots` query hook + `useAddRoot` / `useRemoveRoot` / `useSetRootEnabled` mutations
- 11 unit tests in `db/roots.rs` covering add/remove/list/enable/migration semantics

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Filesystem watcher is not rebuilt on root mutations | `add_root` / `remove_root` after launch | New roots aren't watched until the next launch (file additions to those roots aren't auto-detected). Removing a root leaves a dangling watch. The first rescan covers the immediate state. See `systems/watcher.md`. |
| `paths.path` is stored verbatim (no normalisation) | User picks `/Users/me/Photos/` then later `/Users/me/Photos` (trailing slash) | Two distinct rows because the UNIQUE constraint compares strings literally. Cosmetic — both work, just shows up twice in the Folders list. |
| `add_root` propagates a UNIQUE constraint error as `ApiError::Db` | User adds the same folder twice via add_root | Frontend gets a typed-but-generic DB error. Could be improved to `ApiError::BadInput("already added")`. |
| Removing the only enabled root leaves the user with empty grid + no obvious "add another folder" CTA | Last-root removal | The grid empties cleanly but the empty-state UI uses `pickScanFolder` which goes through `set_scan_root` (replace-all semantic). User has to know to use the Settings drawer's Add Folder button to add additional roots from there. |
| `wipe_images_for_new_root` only fires inside `set_scan_root`, not `add_root` | Legacy NULL-root_id rows persist when only `add_root` is used | Documented; the rows still display because the grid query keeps NULL-root_id rows. Functionally fine. |
| Per-root thumbnail directory removal is best-effort | Filesystem busy / permissions | Logs warn; user can manually clean. The DB rows are gone (CASCADE), so the orphaned files are inert. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Watcher rebuild on root mutations** (cross-cutting with `systems/watcher.md`).
- **Path normalisation at insert time** to deduplicate trailing-slash variants and cross-platform path differences (cross-cutting with `notes/path-and-state-coupling.md`).
- **Specific ApiError for duplicate-path adds** instead of letting the DB UNIQUE error bubble.
- **Per-root scan-priority or include/exclude patterns** — nothing implemented yet, but the schema could grow (`exclude_patterns TEXT NULL`, `priority INTEGER NULL`) without breaking compatibility because the grid query doesn't reference those columns.

## Durable Notes / Discarded Approaches

- **`PRAGMA foreign_keys = ON` is required.** SQLite defaults this OFF for backwards compatibility. Without it, `ON DELETE CASCADE` is a no-op — root removal would leave orphan image rows forever. The pragma is set in `initialize` after every connection open.
- **The `set_scan_root` "replace all" semantic exists but no longer has a frontend caller.** It was originally wired to the top-bar "Choose folder" button under the assumption a no-roots user wanted a clean slate. In practice this silently destroyed previously-added roots when users clicked the button after their first add — the picker looked like an *add* affordance but behaved like a *reset*. The 2026-04-26 fix swapped the button to `add_root` (additive, idempotent on duplicates via a frontend pre-check + backend UNIQUE constraint), renamed it "Add folder", and switched the icon to `FolderPlus` to match. The Tauri command + service test stay in place so the replace-all flow can be reintroduced behind an explicit "Reset library" affordance later without re-implementing the lifecycle.
- **`migrate_legacy_scan_root` is idempotent because settings.json could persist across binary upgrades.** A user who did a legacy → multi-folder migration once should not get duplicate roots if they later downgrade, edit settings.json, and upgrade again.
- **The per-root thumbnail directory layout is a reorg that does not break legacy rows.** `ThumbnailGenerator::generate_thumbnail(path, image_id, None)` writes to the flat layout; `Some(root_id)` writes to the subfolder. The DB stores absolute thumbnail paths so both layouts coexist.
- **Cosine cache is cleared on every root mutation, not selectively pruned.** The cleanup cost would be small (filter out paths whose root_id no longer matches), but the simplicity of "just rebuild from the active DB" is worth more than the milliseconds saved. The cache rebuild is fast (single SELECT + bincode load).

## Obsolete / No Longer Relevant

The pre-Phase-6 model where `settings.json::scan_root` was the single source of truth is gone. The field is preserved for the legacy migration path but never re-set after migration. New installations never write to it.
