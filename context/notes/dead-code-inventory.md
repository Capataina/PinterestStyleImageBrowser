# dead-code-inventory

## Current Understanding

Several files and dependencies exist in the repo with zero runtime use. None are harmful; together they add ~hundred KB of node_modules + several files of mental overhead. A single dead-code-sweep PR could remove all of them.

## Inventory

### Frontend components (unmounted)

| File | Why dead |
|------|----------|
| `src/components/FullscreenImage.tsx` | Earlier inspect view; replaced by `PinterestModal`. |
| `src/components/MasonryItemSelected.tsx` | Replaced by inline `isSelected` path inside `MasonryItem`. |
| `src/components/MasonrySelectedFrame.tsx` | Earlier hero-card frame; replaced by Masonry's hero promotion. |

Verification: `grep -r "FullscreenImage\|MasonryItemSelected\|MasonrySelectedFrame" src/` returns only the file definitions, no imports. The vault notes also mark these as dead.

### Frontend hooks (unimported)

| File | Why dead |
|------|----------|
| `src/hooks/useMeasure.tsx` | Mounts a portal into `#measure-root` for DOM measurement. The infrastructure (the `measure-root` div in `App.tsx`) exists but no caller imports the hook. |

### Backend dead methods

| Symbol | Why dead |
|--------|----------|
| `ImageData::with_thumbnail` (`image_struct.rs:50`) | Alternate constructor; no call sites. The codebase uses `ImageData::new` + manual field assignment everywhere. |
| `db::delete_tag` | Method exists; not registered in `invoke_handler!`; unreachable from frontend. (Different from "dead" in that the function itself is correct — it just has no caller.) |

### Frontend dead query hook

| Symbol | Why dead |
|--------|----------|
| `useSimilarImages` (in `src/queries/useSimilarImages.ts`) | Only `useTieredSimilarImages` is imported. The `useSimilarImages` hook still exists and would call the `get_similar_images` Tauri command, but nothing in the frontend invokes it. |

The Tauri command itself (`get_similar_images`) is registered and would respond — only the React hook wrapper is unused.

### Dead npm dependencies

| Package | Status |
|---------|--------|
| `zustand` | Declared in `package.json:35`, zero `import` sites in `src/`. Carry-over from earlier memory-bank planning that intended to use Zustand for state. TanStack Query took its place. |
| `atropos` | `package.json:21`. Imported as CSS in `App.tsx:3` (`atropos/css`) — the runtime is not used. The 3D tilt is implemented with framer-motion (`MasonryItem.tsx`), not atropos. The CSS adds a few KB to the bundle. |
| `@types/lodash.debounce` | `package.json:20`. Type-only; the actual import is `lodash/debounce` in `Masonry.tsx:4`. Could probably be replaced with `@types/lodash` for consistency, or removed if `lodash`'s own types are good enough. |

## Rationale

Dead code is preserved here as a list of cleanup candidates rather than an urgency. The trade-off:

- **Pro of removing:** smaller node_modules, less code to scan when onboarding, no risk of someone importing a deprecated component.
- **Con of removing:** if any of these were near-future plans (e.g., a settings page that wants `useMeasure`), removing pre-emptively is rework when they come back.

Today nothing in the active roadmap suggests any of these will be needed. A single sweep PR is fine.

## Guiding Principles

- Don't import any of these into new code. If a use case for `useMeasure` or `Atropos` arises, add it back deliberately rather than reviving from corpse.
- The list above is the canonical inventory; if a sweep PR is opened, this note's "Inventory" section is the source of truth for what to remove.
- Verify each removal with `grep` before deletion — past sessions have introduced "dead" markers that were actually live.

## Trigger to revisit

If the inventory grows past ~10 items, the sweep should happen — past that point the noise starts to matter. Today's inventory is below that threshold.
