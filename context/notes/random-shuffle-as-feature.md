# random-shuffle-as-feature

## Current Understanding

The grid order shuffle behaviour changed in Phase 9 — the **default sort mode is now `"added"` (oldest first by id), not random shuffle.** Random shuffle is still available as an explicit user preference but is no longer the default. Two pieces of randomness remain that look like bugs but are intentional UX:

1. **Diversity-pool sampler in `get_similar_images`** picks random `top_n` from the top 20% pool, rather than returning the strict top-N (`cosine/index.rs::get_similar_images`).
2. **The 7-tier sampler in `get_tiered_similar_images`** picks 5 random images per tier from 7 deterministic tiers (0-5%, 5-10%, ..., 40-50%).

The `get_similar_images_sorted` companion exists for cases where ranking accuracy matters more than diversity (semantic search). The split was made deliberately in commit `930f1fc` (2025-12-17).

## What changed (Phase 9)

The pre-Phase-9 backend shuffled on every `get_images_with_thumbnails` call. Combined with progressive thumbnail loading during the indexing pipeline, this caused the visible "entire app refreshes" UX — every refetch (every ~2s while thumbnails were generating) reordered the grid, making tiles jump around.

Resolution:

- Backend always returns sorted-by-id (`db/images_query.rs::get_images_with_thumbnails`).
- Sort modes (`"shuffle" | "name" | "added"`) are now controlled by the user's `useUserPreferences.sortMode` preference and applied frontend-side when needed.
- For `sortMode === "shuffle"`, the frontend applies a deterministic shuffle keyed by `shuffleSeed` (a session-scoped state).
- `shuffleSeed` is bumped on deliberate refresh actions (currently: closing the inspector modal). Indexing-progress invalidations refetch with the SAME seed so the order stays stable through background updates.

Default `sortMode` is `"added"` — the user picks `"shuffle"` deliberately if they want it.

## Why the in-cosine randomness is still a feature

### Diversity-pool sampler

`get_similar_images` does **not** return the strict top-N. It sorts by cosine, takes a pool of `max(top_n, 20% of total)`, then randomly samples `top_n` from that pool.

The reasoning is UX. The strict top-N for visual similarity often produces a result list dominated by near-duplicates (a sequence of slightly cropped versions of the same image, or a series from the same shoot). Sampling within the top 20% guarantees the user sees images that are *actually* similar without seeing them in monotonously decreasing similarity order.

### Tiered sampler within-tier randomness

The 7-tier sampler is the most product-thoughtful piece of the codebase. Per `systems/cosine-similarity.md`:

- Tiers are deterministic: 0-5%, 5-10%, 10-15%, 15-20%, 20-30%, 30-40%, 40-50%.
- Within each tier, 5 images are selected at random.
- A `HashSet<usize>` of used indices ensures no duplicates between tiers.

The within-tier randomness keeps the result feed fresh on repeated views; the tier definition keeps the visual coherence.

## Guiding Principles

- **Do not remove the user-toggleable sort modes** — making `"shuffle"` the default again would regress the Phase 9 fix.
- **Do not "fix" the diversity-pool sampler** in `get_similar_images`. A future refactor that returns the strict top-N because "the most similar should come first" would regress the near-duplicate problem.
- **Do not collapse the tiered sampler into a top-K.** The tier structure is load-bearing UX.
- **Tag mutations and individual image inspections preserve order via the optimistic update pattern** — a tag change does not re-shuffle the grid mid-edit.
- **`get_similar_images_sorted` is the escape hatch** — when ranking-accuracy matters (semantic search), use the sorted method. Don't try to make the diversity sampler do both jobs.

## What was tried

- Pre-Phase-9: backend shuffled on every read. Caused the "entire app refreshes" UX during indexing. Fixed by moving sort to the frontend with explicit user choice.
- The `get_similar_images_sorted` companion was added because semantic search needs deterministic ranking; the diversity sampler is wrong for that use case.

## Trigger to revisit

- Users complain that the grid order is unstable in a way that hurts navigation. Today's stable-by-default `"added"` sort makes this unlikely.
- The grid grows large enough that shuffle is expensive. At 1000 images today, frontend deterministic shuffle is microseconds. At 100k it would still be milliseconds. Not a concern.
- Saved-collection or board features ship — at that point order would need to be persistent within a board, and the global sort modes would need to coexist with per-board ordering.

## Cross-references

- `systems/cosine-similarity.md` § Three retrieval modes
- `systems/database.md` § Stable grid order
- `systems/frontend-state.md` § `useUserPreferences` sortMode
- `systems/search-routing.md` § Shuffle seed coordination
