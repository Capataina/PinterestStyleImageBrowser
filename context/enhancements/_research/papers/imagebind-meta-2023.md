---
source_type: paper
date_published: 2023-05-09
hype_score: 1
---

# ImageBind: One Embedding Space To Bind Them All

## Source reference

- arXiv: https://arxiv.org/abs/2305.05665
- CVPR 2023: https://openaccess.thecvf.com/content/CVPR2023/papers/Girdhar_ImageBind_One_Embedding_Space_To_Bind_Them_All_CVPR_2023_paper.pdf
- Meta Blog: https://ai.meta.com/blog/imagebind-six-modalities-binding-ai/

## Claim summary

Meta's ImageBind: single joint embedding space across **six modalities** — image/video, text, audio, thermal, depth, IMU. Trained with image as the binding modality (other modalities only need image-pair data). Enables cross-modal retrieval like "search images using audio".

## Relevance to our project

A4: Future-direction signal. ImageBind's lineage points toward adding audio/video/depth-aware retrieval *to the same embedding store the project already has*. Far outside current scope but a credible "what comes after CLIP" direction.

A1: Reinforces the "encoder behind a trait" recommendation — the trait should accept any embedding-producing model, including future multimodal ones.

## Specific takeaways

- Out of scope for direct adoption (the project is image-only and intends to stay that way).
- A *post-CLIP* trajectory worth being aware of.
- DINOv2 + ImageBind + SigLIP-2 together suggest the encoder space is differentiating into specialised tracks (image-only, image-text, multimodal).

## Hype indicators

Mild — Meta's blog is promotional but the paper is real.
