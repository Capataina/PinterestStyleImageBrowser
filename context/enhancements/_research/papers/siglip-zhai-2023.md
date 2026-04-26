---
source_type: paper
date_published: 2023-03-27
hype_score: 1
---

# Sigmoid Loss for Language Image Pre-Training (SigLIP)

## Source reference

- arXiv: https://arxiv.org/abs/2303.15343 (ICCV 2023)
- Authors: Xiaohua Zhai, Basil Mustafa, Alexander Kolesnikov, Lucas Beyer (Google DeepMind)
- Date: 2023-03-27
- Venue: ICCV 2023

## Claim summary

Replaces the softmax-normalised contrastive loss in CLIP with a pairwise **sigmoid** loss that does not require a global view of the pairwise similarity matrix. Trains in batches of up to 1M, but also works at small batch sizes — a Large LiT model trained on 4 TPUv4 chips at 20k batch reaches **84.5% ImageNet zero-shot in two days**. Adds learnable temperature + bias parameters.

## Relevance to our project

A1 (Rust+ML systems): SigLIP / SigLIP-2 ONNX exports are drop-in replacements for the OpenAI CLIP ViT-B/32 the project currently ships. The 512-d projection space is preserved (other dims are an option), so the cosine index, BLOB schema, and search code do not change — only the model file does. This is the canonical "swap the encoder, keep everything else" upgrade path.

A4 (Retrieval): SigLIP's better zero-shot retrieval at the same parameter count translates to higher recall@k for the project's semantic-search and "View Similar" paths. Worth measuring as part of any embedding-quality audit.

## Specific takeaways

- SigLIP B/16 at 256² resolution is the most popular drop-in for CLIP B/32 in retrieval tasks; ONNX exports exist on Hugging Face under `google/siglip-base-patch16-256`.
- Multilingual variant (`google/siglip-base-patch16-256-multilingual`) is a direct replacement for the current `clip-ViT-B-32-multilingual-v1` text encoder.
- The sigmoid loss is *training* — at inference, the model is the same kind of dual encoder, no inference-time differences.
- Trainable temperature/bias means the projection space is *similar to* but *not identical to* OpenAI CLIP's; cosine thresholds may need re-calibration.

## Hype indicators

None — peer-reviewed ICCV paper, named lab (Google DeepMind), reproduced widely.
