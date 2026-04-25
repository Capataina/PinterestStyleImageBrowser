# clip-preprocessing-decisions

## Current Understanding

The CLIP image encoder's preprocessing has two known quality concerns that are intentional shortcuts, not unexamined defaults:

| Step | Current value | Reference CLIP | Source |
|------|---------------|----------------|--------|
| Resize filter | `FilterType::Nearest` | Bicubic / Lanczos | `encoder.rs:69` |
| Per-channel mean | `[0.485, 0.456, 0.406]` (ImageNet) | `[0.48145466, 0.4578275, 0.40821073]` (CLIP) | `encoder.rs:91` |
| Per-channel std | `[0.229, 0.224, 0.225]` (ImageNet) | `[0.26862954, 0.26130258, 0.27577711]` (CLIP) | `encoder.rs:92` |

The author's inline comment at `encoder.rs:90` reads: "CLIP-style normalization, we use IMGNET stats here." The substitution is acknowledged in code.

## Rationale

The current preprocessing produces *useful* embeddings — semantic search returns sensible matches and visual similarity finds meaningfully similar images. The accuracy delta vs reference CLIP has not been measured. Running the same image through this pipeline and an OpenAI reference would produce two embedding vectors with cosine similarity near but not identical to 1.0; the resulting semantic-search ranking would be similar but not identical.

The trade-off accepted: visible quality is acceptable for the personal-library use case; engineering cost (one minute of changes) is small but the verification cost (build a comparison harness) is non-trivial, so the change has been deferred.

## Guiding Principles

- This is the cheapest known quality win in the codebase. When the next session decides to invest in embedding quality, swap both: `FilterType::Lanczos3` and the CLIP-native mean/std.
- After swapping, validate by encoding a known image and comparing cosine similarity vs a Python reference — the answer should be ≥ 0.999.
- If the cosine drops below that, there is a deeper preprocessing mismatch (most likely the ONNX export baked in normalisation in a way the export doesn't show).
- Do not silently change one without the other — the two are paired choices.

## What Was Tried

Nothing in version control changed these values. The `Nearest` filter was the original choice from the first encoder commit; the ImageNet stats appear to be a copy-paste from a generic image-classification reference rather than a CLIP-specific one. The author noted the choice but did not revisit.
