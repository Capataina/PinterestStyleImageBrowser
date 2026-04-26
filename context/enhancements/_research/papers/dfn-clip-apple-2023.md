---
source_type: paper
date_published: 2023-09-29
hype_score: 1
---

# Data Filtering Networks (DFN-CLIP)

## Source reference

- arXiv: https://arxiv.org/abs/2309.17425 (ICLR 2024)
- Apple ML Research: https://machinelearning.apple.com/research/data-filtering-networks
- Authors: Alex Fang et al. (Apple)

## Claim summary

DFN-2B / DFN-5B image-text datasets are filtered using neural networks (rather than CLIP-score thresholding). Resulting CLIP-ViT-H trained on DFN-5B reaches **84.4% zero-shot ImageNet** — outperforming LAION-2B, DataComp-1B, and OpenAI's WIT.

## Relevance to our project

A4: The DFN-CLIP weights are public (Hugging Face `apple/DFN5B-CLIP-ViT-H-14-378`) and exportable to ONNX. For the embedding-quality audit recommendation, DFN-CLIP is one of the alternative encoders to compare against in the per-encoder retrieval-quality matrix.

A1: Demonstrates the substance behind "encoder upgrade" as a recommendation — DFN-CLIP, SigLIP-2, MobileCLIP, EVA-CLIP all outperform OpenAI CLIP-ViT-B/32 on retrieval benchmarks.

## Specific takeaways

- For a project with a fixed 512-d schema, DFN-ViT-B variants exist that match the schema.
- The largest DFN-CLIP-ViT-H is too heavy for desktop CPU inference but the smaller variants are tractable.
- License: Apple's DFN weights are available under research-friendly terms.

## Hype indicators

None — peer-reviewed (ICLR), named lab, public weights.
