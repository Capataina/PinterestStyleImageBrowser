---
name: Preprocessing spatial coverage concern
description: Open architectural concern — center-cropping in CLIP/DINOv2 may bias similarity toward image centers, with possible future direction of smart per-query encoder routing
type: project
---

# preprocessing-spatial-coverage

**Status:** open concern, not yet decided. Captured 2026-04-26 so
the next session has the framing without rederiving it.

## The concern

The current preprocessing pipelines do not all see the same part of
each image:

| Encoder | Geometry | What's lost |
|---------|----------|-------------|
| **CLIP** | aspect-preserving resize shortest-edge to 224, then **center-crop 224×224** | Everything outside the central 224×224 region after resize. For tall/wide images the loss is significant. |
| **DINOv2-Base** | aspect-preserving resize shortest-edge to 256, then **center-crop 224×224** | Same shape of loss as CLIP, slightly different sampling. |
| **SigLIP-2** | exact-square resize to 256×256 (bilinear, no crop) | Nothing — every pixel of the original contributes. Aspect distortion is the cost. |

So CLIP and DINOv2 are blind to image edges; SigLIP-2 sees the
whole image but with stretched geometry. These are the canonical
pipelines (matching each model's training-time preprocessing —
deviating from them would degrade embedding quality on that model)
but the user-facing consequence is real.

## Why it matters for this project

The library is heavy on **splash art / character art** style
images. In that domain, the center is often a character or focal
subject, but the periphery carries:

- scenery and atmosphere (forest edge, sky, water)
- secondary characters
- stylistic frames / borders
- color washes and lighting effects

A search like "green forest" or "blue sky" against a CLIP-encoded
library could miss images where the green/blue is dominant in the
periphery but the center is a non-green/non-blue character. The
encoder simply never saw the green forest.

SigLIP-2 should handle this kind of color/scenery query better
because it sees the full image — even with stretching, the color
distribution is preserved.

## Possible directions (NOT yet decided)

1. **Accept the trade-off** — keep canonical pipelines, document
   the limitation in the encoder picker UI ("Use SigLIP-2 for
   color/scenery queries; CLIP for character/object queries").
2. **Smart per-query routing** — frontend (or backend) inspects
   the query type and chooses an encoder:
   - color words ("blue", "green"), scenery words → SigLIP-2
   - character names, objects, concepts → CLIP
   - "find more like this image" → DINOv2
3. **Query-time multi-encoder fusion** — run the query through
   every encoder, fuse rankings (e.g. reciprocal-rank fusion).
   More expensive but no routing decision needed.
4. **Tile-based encoding** — encode each image as multiple crops
   (center + four corners + full-square-stretched) and store all
   embeddings. Search ranks by max-over-tiles. ~5× storage, but
   solves the spatial-blindness problem on every encoder.
5. **Switch CLIP/DINOv2 to no-crop preprocessing** — pad rather
   than crop, or stretch like SigLIP-2 does. Deviates from
   canonical and would silently degrade embedding quality on
   anything trained with center-crop expectations.

The user's instinct in conversation: option 2 (smart routing) is
the most appealing direction long-term — leverage every encoder
for what it does best rather than picking one global default. But
no decision yet.

## What would inform the decision

- A diagnostic that runs a known "color in periphery only" query
  through every encoder and shows side-by-side rankings. The
  `cross_encoder_comparison` diagnostic added this session is
  the foundation; we'd need a few hand-labeled query/expected-
  match pairs to make it interpretable.
- Watching the user's actual search patterns — if color queries
  are common and produce bad results on CLIP, that pushes harder
  for option 2 or 4.
- Storage cost estimate for option 4 (tile-based).

## Why we are not blocked

The system works without resolving this. The encoder picker lets
the user choose per-query (image vs text), and SigLIP-2 is wired
in as a top-level option. A motivated user can pick the right
encoder for the query manually. Smart routing would be a
quality-of-life upgrade, not a fix for something broken.
