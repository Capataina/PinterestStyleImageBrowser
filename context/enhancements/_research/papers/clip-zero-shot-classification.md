---
source_type: paper
date_published: 2021-02-26
hype_score: 1
---

# CLIP — Connecting Text and Images (Zero-Shot Classification)

## Source reference

- OpenAI Blog: https://openai.com/index/clip/
- Pinecone tutorial: https://www.pinecone.io/learn/series/image-search/zero-shot-image-classification-clip/
- Original CLIP paper: Radford et al. 2021 (https://arxiv.org/abs/2103.00020)

## Claim summary

CLIP enables zero-shot image classification by encoding candidate labels as text prompts ("a photo of a {label}") and ranking by cosine similarity. No labelled training data needed. Applied to a fixed label dictionary, this becomes auto-tagging.

## Relevance to our project

A4 + A1: The project's M6 Roadmap explicitly lists "Auto-tagging via CLIP zero-shot classification". The infrastructure is already in place — text encoder, cosine index, tag system. Adding zero-shot auto-tagging is one Tauri command + a tag dictionary. This is a *direct* additive recommendation downstream.

## Specific takeaways

- Auto-tagging maps every image's embedding against a fixed set of label embeddings, assigning tags above a confidence threshold.
- The tag dictionary itself is the design choice — generic ImageNet labels vs project-relevant categories ("portrait", "landscape", "street", "macro", etc.).
- The threshold is the user-tunable parameter — too low: noise; too high: empty tags.

## Hype indicators

Mild — Pinecone's tutorial is marketing-shaped, but CLIP itself is OpenAI's published research.
