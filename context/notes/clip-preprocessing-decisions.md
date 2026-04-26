---
name: CLIP preprocessing decisions
description: History and current state of CLIP image-encoder preprocessing — what was wrong, what was fixed, what the trade-offs were
type: project
---

# clip-preprocessing-decisions

## Current state (2026-04-26)

CLIP image preprocessing now matches the canonical OpenAI / Xenova
pipeline:

| Step | Value | Source |
|------|-------|--------|
| Resize filter | `FilterType::CatmullRom` (bicubic-family — closest match to PIL's `BICUBIC`) | `encoder.rs` |
| Resize geometry | Aspect-preserving shortest-edge → 224, then center-crop 224×224 | `encoder.rs` |
| Per-channel mean | `[0.48145466, 0.4578275, 0.40821073]` (CLIP-native) | `encoder.rs` |
| Per-channel std | `[0.26862954, 0.26130258, 0.27577711]` (CLIP-native) | `encoder.rs` |
| L2 normalize on output | yes | `encoder.rs` |

Replaced the prior `resize_exact(224, 224)` + `Lanczos3` + ImageNet
mean/std + no-output-normalize pipeline. The migration
`migrate_embedding_pipeline_version` (version 2) wipes legacy CLIP
embeddings on first launch so the next indexing pass re-encodes
under the new pipeline.

## Why the old pipeline was a problem

- **`resize_exact(224, 224)` squashed non-square images** — CLIP was
  trained on aspect-preserving resize + center-crop. Embeddings of
  squashed images shifted away from what reference CLIP would
  produce.
- **ImageNet mean/std subtly skewed normalization** — every channel's
  mean/std differed from the values CLIP was trained on, biasing
  the embedding distribution.
- **No L2 normalize** meant cosine had to divide by norms every call;
  worked but masked the fact that the embedding magnitudes drifted
  off the unit sphere.

## What replaced it (and what stayed open)

Fixed: bicubic-family resize, aspect-preserving geometry,
CLIP-native normalization stats, L2-normalize output. These are now
consistent with `Xenova/clip-vit-base-patch32`'s
`preprocessor_config.json`.

**Still open:** the center-crop step itself drops everything outside
the central 224×224 region. For images where meaningful content
sits at the edges (splash arts with edge scenery, group photos,
landscapes with foreground at the bottom), the encoder never sees
that content. See `preprocessing-spatial-coverage.md` for the
concern and possible directions.

## Validation

The diagnostic stack now emits `preprocessing_sample` (per-encoder
first-image L2 norm + range + NaN counts), `embedding_stats`
(per-encoder L2-norm distribution across the cache), and
`pairwise_distance_distribution` (50-sample histogram). These
together replace the "build a comparison harness vs Python
reference" deferred work — quality issues surface in the report
without a separate validation tool.
