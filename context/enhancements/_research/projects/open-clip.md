---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# OpenCLIP (mlfoundations / LAION)

## Source reference

- GitHub: https://github.com/mlfoundations/open_clip
- Pretrained models docs: https://github.com/mlfoundations/open_clip/blob/main/docs/PRETRAINED.md
- Scaling-laws paper: https://arxiv.org/abs/2212.07143 (CVPR 2023)

## Claim summary

The reference open-source CLIP implementation. Models trained on LAION-400M, LAION-2B, DataComp-1B. Companion paper: "Reproducible Scaling Laws for Contrastive Language-Image Learning" (Cherti, Beaumont, Schuhmann et al., CVPR 2023).

## Relevance to our project

A1 + A4: OpenCLIP is the canonical *Python* reference the project's embedding-quality audit should compare against (per `clip-preprocessing-decisions.md` line 24: "validate by encoding a known image and comparing cosine similarity vs a Python reference — the answer should be ≥ 0.999").

## Specific takeaways

- Many of the alternative encoders (SigLIP, EVA-CLIP, MobileCLIP, DFN-CLIP) are accessible via OpenCLIP's loader API.
- The scaling-laws paper is the canonical reference for "is bigger CLIP better?" — informs which size to recommend.
- LAION-2B and DataComp-1B are the two leading public training sets.

## Hype indicators

None — academic OSS project.
