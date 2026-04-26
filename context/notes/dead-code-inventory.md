# dead-code-inventory

## Current Understanding

The previous inventory was largely closed by the audit Pass + Phase 2 dead-code sweep. Most flagged items have been removed or wired up. This note now serves as the residual list and the trigger for the next sweep.

## Resolved (since previous inventory)

| Item | Status | When |
|------|--------|------|
| `src/components/FullscreenImage.tsx` | Removed | Phase 2 (commit `86df34e`) |
| `src/components/MasonryItemSelected.tsx` | Removed | Phase 2 |
| `src/components/MasonrySelectedFrame.tsx` | Removed | Phase 2 |
| `src/hooks/useMeasure.tsx` | Removed | Phase 2 |
| `db::delete_tag` (orphaned method) | Wired through `commands::tags::delete_tag` + `useDeleteTag` mutation | Phase 6 |
| `useSimilarImages` (frontend hook) | Removed (only `useTieredSimilarImages` is used) | Audit dead-code sweep |
| `ImageData::with_thumbnail` (alternate constructor) | Removed | Audit Dead-Code finding |
| `add_tag_to_image` plain INSERT (would error on duplicate) | Hardened to `INSERT OR IGNORE` | Phase 6 |
| Hardcoded `Path::new("test_images")` in `main.rs` | Removed (multi-folder pipeline) | Phase 6 |
| `unsafe { slice::from_raw_parts(...) }` for embedding BLOB casts (3 sites) | Replaced with `bytemuck::cast_slice` | Audit `0bdb5f4` |
| Triplicated `normalize_path` closure in `lib.rs` (3 sites) | Extracted into `paths::strip_windows_extended_prefix` | Audit `02b12b9` |
| Triplicated 3-strategy DB-id lookup blocks (3 sites) | Extracted into `commands::resolve_image_id_for_cosine_path` | Same audit commit |
| Duplicated `aggregate_image_rows` pattern (4 sites) | Extracted into `db/images_query.rs::aggregate_image_rows` helper | Audit `a30c153` |
| `[Backend] ...` `println!` logging convention | Replaced wholesale by `tracing::info!` / `debug!` / `warn!` | Phase 6 |
| `set_scan_root` Tauri command (single-folder model) | Preserved as legacy; multi-folder commands added (`add_root` / `remove_root` / `set_root_enabled`) | Phase 6 |
| `models/` "user-supplied" assumption | Now auto-downloaded on first launch via `model_download` | Phase 4b |

## Residual (current dead-code inventory)

### Backend

| Item | Status | Reason to keep |
|------|--------|----------------|
| `Encoder::inspect_model` | Defined; not called from runtime code | Useful for debugging. Could be moved behind `#[cfg(test)]` if not needed in production. |
| `CosineIndexState.db_path: String` field | Stored in state but the cosine module no longer reads it (audit fix) | Still needed by the indexing pipeline + commands::roots for spawning their own `ImageDatabase`. Could be routed differently and the field dropped, but currently load-bearing. |
| `Settings::scan_root` field | Read by lib.rs setup callback for legacy migration; cleared after | Required for the one-shot legacy migration path. Cannot be removed until enough time passes that no user has the field populated. Effectively immortal. |

### Frontend

| Item | Status | Reason to keep |
|------|--------|----------------|
| `setIsInspecting` state in `[...slug].tsx` | Verification needed: appears in head section but consumption is not visible | Marked as a Coverage gap in `architecture.md`. Either prune or document the consumption site. |

### Dependencies

| Package | Status | Reason / action |
|---------|--------|-----------------|
| `zustand` | Declared in `package.json`, zero `import` sites in `src/` | Carry-over from earlier memory-bank planning. Safe to remove. |
| `atropos` | Imported as CSS in `App.tsx` (`atropos/css`); the runtime is not used | The 3D tilt is implemented with framer-motion, not atropos. The CSS adds a few KB. Safe to remove the CSS import + the dep. |
| `@types/lodash.debounce` | Imported via `lodash/debounce` in `Masonry.tsx` | Type-only. Could swap for `@types/lodash` for consistency, or remove if `lodash`'s own types are good enough. Low priority. |

## Rationale

The bulk of the previous inventory was closed in two waves: Phase 2's dead-code sweep + Phase 6's wiring of orphaned methods + the audit's modularisation/extraction findings. The residual list is small and not urgent.

## Guiding Principles

- **Don't import any of the removed items into new code.** If a use case for one arises, add it back deliberately rather than reviving from corpse.
- **The list above is the canonical inventory** — if a small sweep PR is opened, this section is the source of truth for what to remove.
- **Verify each removal with `Grep` before deletion** — past sessions have introduced "dead" markers that were actually live (e.g., `useSimilarImages` was flagged but a future change might re-import it).

## Trigger to revisit

When the residual list grows past ~5 items again, schedule a small sweep PR. Today's residual is below that threshold.
