# random-shuffle-as-feature

## Current Understanding

Two pieces of randomness in this codebase look like bugs but are intentional UX:

1. **Catalog grid shuffles on every `get_images` call** (`db.rs:496-499`).
2. **Diversity-pool sampler in `get_similar_images`** picks random `top_n` from the top 20% pool, rather than returning the strict top-N (`cosine_similarity.rs:140-155`).

Plus a third intentional randomisation in the tiered sampler (the within-tier random selection — but the *tiers* themselves are deterministic).

## Rationale

### Catalog shuffle

Per commit `36b33b66` (2025-12-17): "Images are now shuffled randomly instead of being sorted by ID. ... The frontend now invalidates the images query on modal close to ensure a new random order is fetched."

The reasoning is feed freshness. Without it, returning to the catalog after exploring shows the same first-tile, second-tile, third-tile sequence every time and the grid feels static. With it, every modal close refreshes the grid order, making re-visits feel like discovery.

The pairing of backend shuffle + frontend `invalidateQueries(["images"])` on modal close is what makes this work — without the explicit invalidation, the cached query would survive and the order would be stable for the session.

### Diversity-pool sampler

`get_similar_images` does **not** return the strict top-N. It sorts by cosine, takes a pool of `max(top_n, 20% of total)`, then randomly samples `top_n` from that pool.

The reasoning is also UX. The strict top-N for visual similarity often produces a result list dominated by near-duplicates (a sequence of slightly cropped versions of the same image, or a series from the same shoot). Sampling within the top 20% guarantees the user sees images that are *actually* similar without seeing them in monotonously decreasing similarity order.

The companion `get_similar_images_sorted` exists for cases where ranking accuracy matters more than diversity (semantic search). The split was made deliberately in commit `930f1fc` (2025-12-17).

### Tiered sampler within-tier randomness

The 7-tier sampler is the most product-thoughtful piece of the codebase. Per `systems/cosine-similarity.md`:

- Tiers are deterministic: 0-5%, 5-10%, 10-15%, 15-20%, 20-30%, 30-40%, 40-50%.
- Within each tier, 5 images are selected at random.
- A `HashSet<usize>` of used indices ensures no duplicates between tiers.

The within-tier randomness keeps the result feed fresh on repeated views; the tier definition keeps the visual coherence.

## Guiding Principles

- **Do not "fix" the catalog shuffle.** A future refactor that switches to `ORDER BY id` because "shuffling looks random/wrong" would regress feed freshness.
- **Do not "fix" the diversity-pool sampler.** A future refactor that returns the strict top-N because "the most similar should come first" would regress the near-duplicate problem.
- **Do not collapse the tiered sampler into a top-K.** The tier structure is load-bearing UX.
- **Tag mutations and individual image inspections preserve order via the optimistic update pattern** — a tag change does not re-shuffle the grid mid-edit. The shuffle only happens on `invalidateQueries`, which is currently only on modal close.
- **`get_similar_images_sorted` is the escape hatch** — when ranking-accuracy matters (semantic search), use the sorted method. Don't try to make the diversity sampler do both jobs.

## What was tried

Earlier code sorted by id (deterministic). The shift to shuffle was a single commit with a clear rationale in the body. There is no evidence of an earlier strict-top-N similarity that was replaced — the diversity-pool sampler is original.

## Trigger to revisit

- Users complain that the grid order is unstable in a way that hurts navigation (they wanted to remember "the third image from the right was X" and it moved). Today this is unlikely because each grid session lasts seconds, not minutes.
- The grid grows large enough that shuffle is expensive. At 749 images today, `rand::rng().shuffle()` is microseconds. At 100k it would still be milliseconds. Not a concern.
- Saved-collection or board features ship — at that point order would need to be persistent within a board, and the global shuffle would need to coexist with per-board ordering.
