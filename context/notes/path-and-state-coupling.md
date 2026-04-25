# path-and-state-coupling

## Current Understanding

The codebase has two related concerns that share a root cause: paths are not normalised at insert time, and the cosine module owns its own DB connection. The downstream effect is a triplicated `normalize_path` closure in `lib.rs` and a multi-strategy fallback for mapping cosine-result paths back to DB ids.

## The two concerns

### 1. The `populate_from_db(db_path: &str)` signature

`CosineIndex::populate_from_db` opens a *second* `ImageDatabase` connection from a stored path string rather than borrowing the existing managed `&ImageDatabase`:

```rust
pub fn populate_from_db(&mut self, _db_path: &str) {
    let db = db::ImageDatabase::new(_db_path).expect("failed to init db");
    // ...
}
```

`cosine_similarity.rs:22-61`. The `CosineIndexState` struct (`lib.rs:38-41`) stores the `db_path: String` alongside the index for this purpose.

### 2. The triplicated `normalize_path` closure

The closure that strips the Windows `\\?\` extended-path prefix is defined inline inside three Tauri commands: `semantic_search`, `get_similar_images`, `get_tiered_similar_images`. (`lib.rs:182-188`, `:467-475`, `:308-315`.)

After the strip, each command does up to 3 lookup strategies — exact match on normalised path, exact match on original path, flexible match against all images comparing canonicalised paths.

## Rationale

Both concerns trace back to one decision: paths are stored in `images.path` exactly as they are produced by `std::fs::read_dir`, with no normalisation. On Windows this can produce `\\?\C:\foo\bar` paths in some contexts and `C:\foo\bar` in others. When the cosine index later returns a `PathBuf`, that may or may not match the path stored in the DB.

The path-normalisation strategy was added in commit `2606854` (2025-12-12): "Enhanced the logic for mapping image paths to IDs by normalizing Windows extended path prefixes, attempting both normalized and original formats, and adding a flexible fallback that compares canonicalized paths against all images in the database. This increases robustness when handling various path representations and improves matching accuracy."

The flexible fallback works in practice. The cost is the triplicated closure and the multi-strategy lookup that runs for every result of every cosine call.

## Guiding Principles

The cleanest fix is to normalise paths at insert time. Concretely:

1. Add a `normalize_for_storage(p: &Path) -> String` helper to `filesystem-scanner` or a new `paths.rs` module that strips `\\?\`, canonicalises if the file exists, and returns a `String`.
2. Apply it inside `ImageScanner::scan_directory` so the strings handed to `db.add_image` are already canonical.
3. Rebuild the existing `images.db` (or migrate paths once) so historical rows match.
4. After that, the cosine module can be refactored to take `&ImageDatabase` (passing path keys directly) and the multi-strategy lookup collapses to a single `db.get_image_id_by_path(path)`.

Until that work is done:

- Don't add a fourth normalisation closure. Either reuse the existing pattern via copy-paste *or* extract a single module-level helper.
- The cosine module's `_db_path: &str` argument intentionally has the underscore prefix — the author signalled that the parameter is awkward but not yet refactored. Honour that signal.

## What Was Tried

Earlier code (pre-commit `2606854`) used a single-strategy lookup that broke on Windows. The flexible fallback was added explicitly to cover the corner case rather than fixing the underlying path-storage problem. The trade-off was: fixing storage required a migration; adding fallback was a self-contained PR.

## Trigger to revisit

If any of the following happens:
- A second user reports "results disappear after a similar-images query" (which would be a path-mapping miss).
- The codebase grows a fourth call site that needs path normalisation.
- A future feature needs to compare paths from another source (e.g., user-supplied filter) against DB paths.

…then the right intervention is normalise-at-insert + drop the multi-strategy fallback. Until then, the current flexible-match pattern is good enough.
