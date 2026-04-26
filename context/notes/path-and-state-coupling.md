# path-and-state-coupling

## Current Understanding

The codebase had two related concerns that shared a root cause: paths were not normalised at insert time, and the cosine module owned its own DB connection. The audit closed one half of the problem (the cosine module no longer holds its own connection; the triplicated `normalize_path` closure is now a single helper). The other half — normalising paths at insert time — is still pending.

## What changed (audit fixes shipped)

1. **`CosineIndex::populate_from_db` now takes `&ImageDatabase`.** The previous signature constructed a fresh `ImageDatabase` from a stored path string; the cosine module owned a duplicate connection. Audit `ae0006d` + `5c2b0f6` collapsed this to a borrow.
2. **The triplicated `normalize_path` closure was extracted into `paths::strip_windows_extended_prefix(&str) -> Cow<'_, str>`.** Returns `Cow::Borrowed` on the common path (no allocation when the prefix is absent). Audit `02b12b9`.
3. **The 3-strategy DB-id lookup was extracted into `commands::resolve_image_id_for_cosine_path(db, path, all_images_cache)`.** Three previous 60-line duplicate blocks across `semantic_search`, `get_similar_images`, `get_tiered_similar_images` are now one call. Same audit commit.

## What's still pending

**Paths are still stored in `images.path` exactly as they are produced by `std::fs::read_dir`, with no normalisation at insert time.** On Windows this can produce `\\?\C:\foo\bar` paths in some contexts and `C:\foo\bar` in others. When the cosine index later returns a `PathBuf`, that may or may not match the path stored in the DB. The 3-strategy fallback in `resolve_image_id_for_cosine_path` handles it but has its own cost (per-call scan of `all_images_cache` in the worst case).

The path-normalisation strategy was added in commit `2606854` (2025-12-12): "Enhanced the logic for mapping image paths to IDs by normalizing Windows extended path prefixes, attempting both normalized and original formats, and adding a flexible fallback that compares canonicalized paths against all images in the database. This increases robustness when handling various path representations and improves matching accuracy."

The flexible fallback works in practice. The cost is the multi-strategy lookup that runs for every result of every cosine call.

## Guiding Principles

The cleanest fix is to normalise paths at insert time. Concretely:

1. Add a `normalize_for_storage(p: &Path) -> String` helper to `filesystem-scanner` or `paths.rs` that strips `\\?\`, canonicalises if the file exists, and returns a `String`.
2. Apply it inside `ImageScanner::scan_directory` so the strings handed to `db.add_image` are already canonical.
3. Migrate the existing `images.db` (or backfill paths once) so historical rows match.
4. After that, the 3-strategy fallback in `resolve_image_id_for_cosine_path` collapses to strategy 1 only.

Until that work is done:

- **Don't add another normalisation closure or path-stripping site.** The single `paths::strip_windows_extended_prefix` helper is the source of truth.
- **The cosine module no longer needs the `db_path: String` field on `CosineIndexState`** — it's only kept for the indexing pipeline's separate-thread `ImageDatabase::new`. A future cleanup could route that through a different mechanism and drop the field.

## What Was Tried

Earlier code (pre-commit `2606854`) used a single-strategy lookup that broke on Windows. The flexible fallback was added explicitly to cover the corner case rather than fixing the underlying path-storage problem. The trade-off was: fixing storage required a migration; adding fallback was a self-contained PR.

The cosine module's 2-connection pattern was preserved across multiple sessions because changing the API required changing every call site. The audit Modularisation pass changed enough call sites at once that it was natural to also change the API at that point — the DB submodule split + the cosine submodule split + the call-site updates landed together.

## Trigger to revisit

If any of the following happens:
- A second user reports "results disappear after a similar-images query" (a path-mapping miss).
- A future feature needs to compare paths from another source (e.g., user-supplied filter) against DB paths.
- Profile data shows the 3-strategy fallback is on the hot path.

…then the right intervention is normalise-at-insert + drop the multi-strategy fallback. Until then, the current pattern is good enough.

## Cross-references

- `systems/database.md` § Known Issues for the broader path-comparison risk.
- `systems/paths-and-state.md` § Implementation for the single helper.
- `systems/tauri-commands.md` § `resolve_image_id_for_cosine_path` for the consumer pattern.
