---
source_type: shipped-project
date_published: 2025-02-21
hype_score: 0
---

# SigLIP-2 HuggingFace Model Card

## Source reference

- HF blog: https://huggingface.co/blog/siglip2
- Base model: https://huggingface.co/google/siglip2-base-patch16-224
- timm export: https://huggingface.co/timm/ViT-B-16-SigLIP2

## Claim summary

Released **2025-02-20**. Four model sizes: ViT-B/86M, L/303M, So400m/400M, g/1B. Both NaFlex (variable resolution, native aspect ratio) and FixRes (backwards-compat with SigLIP-1) variants. **Outperforms SigLIP-1 at all model scales** on zero-shot classification, image-text retrieval, transfer learning.

## Relevance to our project

A1 + A4: The base ViT-B variant is a *direct drop-in* for the project's current OpenAI CLIP-ViT-B/32. Embedding dim is the same (768 → 512 projection); the cosine index is unchanged.

A2: License is Apache 2.0 — no licensing block.

## Specific takeaways

- The "FixRes" variant is the easier swap (preserves the existing 224×224 preprocessing); NaFlex gives quality wins but requires preprocessing changes.
- HF provides ONNX exports via `optimum` toolkit.
- Multilingual variant exists for the text encoder swap.

## Hype indicators

None — official HuggingFace + Google model cards.
