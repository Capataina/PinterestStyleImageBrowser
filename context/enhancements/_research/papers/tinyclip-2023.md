---
source_type: paper
date_published: 2023-09-21
hype_score: 1
---

# TinyCLIP: CLIP Distillation via Affinity Mimicking and Weight Inheritance

## Source reference

- ICCV 2023: https://openaccess.thecvf.com/content/ICCV2023/papers/Wu_TinyCLIP_CLIP_Distillation_via_Affinity_Mimicking_and_Weight_Inheritance_ICCV_2023_paper.pdf
- arXiv: https://arxiv.org/abs/2309.12314
- GitHub: https://github.com/wkcn/TinyCLIP

## Claim summary

Distills CLIP to ~50% size while maintaining zero-shot accuracy via affinity mimicking + weight inheritance. TinyCLIP-ViT-8M/16 reaches 41.1% ImageNet zero-shot, 3.5% above CLIP-ViT-B/16 at 8.9% of parameters. Training is 1.4-7.8× faster than from-scratch.

## Relevance to our project

A1 + A4: Another path to a smaller / faster CLIP encoder for the local-first stack. TinyCLIP is an alternative to MobileCLIP — both target the on-device-CLIP problem. For a comparison-of-encoders audit, TinyCLIP belongs in the matrix.

## Specific takeaways

- TinyCLIP weights on Hugging Face under `wkcn/TinyCLIP-*`.
- Distillation work is generally publishable directly to bench reports; comparing TinyCLIP / MobileCLIP / SigLIP-B / OpenAI CLIP-B/32 on the project's `test_images/` would be a small, citable artefact.

## Hype indicators

None — peer-reviewed ICCV.
