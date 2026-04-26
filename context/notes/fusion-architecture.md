# Fusion architecture

How the multi-encoder system works end-to-end. Written so a future session (or a returning user) can grasp the model without re-deriving it from code.

## The two loops

Every encoder participates in two distinct loops. Conflating them is the most common confusion source.

### Loop 1 — Indexing (background, per image, runs once)

For each image newly added to the library:

1. Generate a thumbnail (`thumbnail-pipeline`).
2. Run **every enabled image encoder** on the source image. As of Phase 11e they run **in parallel** (one thread per encoder, each with its own `ImageDatabase` connection). Each encoder produces one vector and writes it to `embeddings(image_id, encoder_id, embedding)`.
3. Text encoders are **not** run during indexing. They encode the user's *query*, not the user's library.

So after indexing, the `embeddings` table has up to (`#enabled_image_encoders` × `#images`) rows — one per (image, encoder) pair.

### Loop 2 — Search (foreground, per user action, runs every time)

**Image-to-image** (clicked an image to find similar ones):

1. For each enabled image encoder, look up the clicked image's vector in that encoder's space.
2. For each encoder, score the query vector against every other image in the cosine cache for that encoder. Get a ranked list (top-K).
3. Fuse the ranked lists via Reciprocal Rank Fusion (RRF). The single fused list is what the user sees.

**Text-to-image** (typed a query):

1. For each **enabled text-supporting** encoder (CLIP, SigLIP-2 — DINOv2 has no text branch and is implicitly skipped), encode the query into a text vector.
2. For each, score against the matching image-side cosine cache. Get a ranked list (top-K).
3. RRF fuse → single ordered list.

## Why fusion replaces the picker

The previous design had a single "active" encoder per direction (image-image and text-image) with a dropdown to pick between them. Two problems:

1. **No single encoder is best.** CLIP cares about concept overlap. DINOv2 cares about visual / structural similarity. SigLIP-2 cares about descriptive content. Different queries need different lenses.
2. **Picking one disables the others.** Every other encoder's embeddings sit unused. Wasted indexing time, wasted disk.

Fusion solves both: every enabled encoder contributes to every search. The fused score rewards consensus (an image all three encoders rank highly is the strongest match) while preserving each encoder's unique signal.

## Why RRF specifically (not score-fusion)

Score-fusion (sum-or-mean of normalised cosines) sounds simpler but is fragile:

- Different encoders produce cosines on different distributions. CLIP's "0.85" is not comparable to DINOv2's "0.85"; one might be a near-perfect match and the other a weak hit.
- L2 normalisation alone doesn't fix this; the underlying distributions differ.

RRF discards the score and only uses the **rank**. Encoder distributions don't matter — only "did this encoder rank this image highly relative to its other results?". The math is `score = Σ 1 / (k + rank)` over encoders; `k = 60` is the canonical value from Cormack et al. 2009.

## Why per-encoder enable/disable, not always-all

Not every user wants the cost of all 3 encoders. Each adds:

- ~350-500 MB of model weights on disk
- ~1/3 of the indexing time
- ~6 MiB of RAM per 2000 images for the resident cosine cache

Toggling an encoder off:

- Skips it on the next indexing pass (existing rows stay).
- Excludes it from fusion (the next fusion call only iterates over enabled encoders).

Toggling back on:

- **Existing rows are instantly available** for fusion. No re-encode.
- New (unencoded) images get encoded on the next indexing pass.

This is why disabling is non-destructive: embeddings are never deleted from the DB just because the encoder is toggled off. The data is cheap to keep, expensive to regenerate.

## Where the canonical state lives

- `settings.json::enabled_encoders` — the source of truth, persisted on disk in the app data directory.
- IPC `get_enabled_encoders` / `set_enabled_encoders` — frontend reads + mutates.
- Indexing pipeline reads this list at spawn and only runs enabled encoders.
- `get_fused_similar_images` (image-image) and `get_fused_semantic_search` (text-image) read this list per call.
- Frontend `EncoderSection` mirrors it as toggle state for UX, but the backend is authoritative.

## Why text-image fusion is asymmetric (only 2 encoders, not 3)

DINOv2 is image-only — there is no DINOv2 text encoder. So text-image fusion can use at most 2 encoders (CLIP, SigLIP-2), even when 3 image encoders are enabled.

That's fine — RRF works with any number of ranked lists ≥ 1. With only 1 enabled text encoder, the fused output equals the single-encoder output (unstable sort can swap ties but otherwise identical).

## Lifecycle: what happens when

| User action | Frontend | Backend | Indexing |
|---|---|---|---|
| First launch | `get_enabled_encoders` returns default (all 3) | settings.json doesn't yet have the field | First pass encodes for all 3 |
| Click an image (View Similar) | `useTieredSimilarImages` → `fetchFusedSimilarImages` | `get_fused_similar_images` reads enabled list, fuses | Not triggered |
| Type a query | `useSemanticSearch` → `fetchFusedSemanticSearch` | `get_fused_semantic_search` reads enabled list (intersected with text-capable encoders), fuses | Not triggered |
| Toggle an encoder OFF | `EncoderSection` calls `set_enabled_encoders` | settings.json updated; future fusion calls skip this encoder | Existing rows stay; the encoder doesn't run on the next pass |
| Toggle an encoder back ON | Same | Future fusion calls re-include this encoder; **existing rows resurrect instantly** | Encoder runs only for images that don't yet have a row for it |
| Add a new folder | `add_root` IPC | Cosine + fusion caches invalidated | Pipeline runs every enabled encoder on the new images |

## Performance shape

After Tier 1 + Tier 2 + Phase 11c+e, indexing wall-clock is roughly:

```
parallel( CLIP encode ‖ SigLIP-2 encode ‖ DINOv2 encode )
    ≈ max(CLIP, SigLIP-2, DINOv2) ≈ SigLIP-2 (slowest)
```

Compared to the previous serial CLIP → SigLIP-2 → DINOv2 chain (sum of all three), parallel saves ~half the encoder phase wall-clock for full-3 indexing. Fewer encoders = strictly fewer threads spawned; with 1 encoder enabled, the parallelism overhead is zero.

Search-side: each enabled encoder does one cosine score pass + one sort. With caches resident in `FusionIndexState`, that's ~5-15 ms per encoder, ~15-45 ms total fused. Plus RRF is O(N log N) on the union of top-K's — negligible.

## Adding a 4th encoder later

The system is plug-and-play once an encoder implements the `ImageEncoder` (and optionally `TextEncoder`) trait. To add one:

1. Implement the encoder module + register an `EncoderInfo` in `commands::encoders::ENCODERS`.
2. Add its branch to `indexing.rs::run_encoder_phase` (the `match encoder_id.as_str()` block inside the spawned thread).
3. Add the model URL + filename to `model_download.rs`.
4. Bump `CURRENT_PIPELINE_VERSION` if the new encoder's preprocessing changes any embedding distribution.

Fusion automatically picks it up because the iteration is over `enabled_encoders`. The user toggles it on in Settings, the next indexing pass encodes for it, and the next fusion call includes its ranked list.

## See also

- `systems/multi-encoder-fusion.md` — implementation details (RRF math, per-encoder cache state, IPC shape).
- `notes/encoder-additions-considered.md` — research-grade notes on what 4th encoder we might add.
- `notes.md` § Active work areas — Phase 5 (image-image fusion) was part of the broader perf bundle; the per-recommendation plan was deleted post-ship.
