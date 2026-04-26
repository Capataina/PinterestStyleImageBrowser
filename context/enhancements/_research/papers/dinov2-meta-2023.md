---
source_type: paper
date_published: 2023-04-14
hype_score: 1
---

# DINOv2: Learning Robust Visual Features without Supervision

## Source reference

- arXiv: https://arxiv.org/abs/2304.07193
- Meta AI Blog: https://ai.meta.com/blog/dino-v2-computer-vision-self-supervised-learning/
- GitHub: https://github.com/facebookresearch/dinov2
- DINOv3 (2024 successor): https://ai.meta.com/blog/dinov3-self-supervised-vision-model/

## Claim summary

Self-supervised ViT trained on 142M images via self-distillation. 1B-param ViT distilled into smaller models that surpass OpenCLIP on most image and pixel-level benchmarks. **Image-only** features (no text alignment), so cannot do text-image semantic search out-of-the-box.

## Relevance to our project

A4 (Retrieval): DINOv2 features are *better than CLIP* for **image-only** similarity tasks. The project's "View Similar" feature could swap to DINOv2 for higher-quality similarity, while keeping CLIP for the text-image semantic search path. This is a direct additive recommendation: dual-encoder system, picked per query type.

A1: DINOv2 has ONNX exports available and weight sizes range from 21M to 1.1B parameters — small variants are tractable for desktop CPU.

## Specific takeaways

- Image-image similarity: DINOv2 > CLIP per published benchmarks.
- Text-image semantic search: still CLIP / SigLIP territory.
- DINOv3 (2024) further improves over DINOv2.
- Plug-and-play: load DINOv2 ONNX as a separate encoder; route "View Similar" queries through DINOv2 and "semantic_search" queries through CLIP/SigLIP.
- License: Apache 2.0.

## Hype indicators

Mild — Meta's blog is promotional but the underlying paper and code are substantive.
