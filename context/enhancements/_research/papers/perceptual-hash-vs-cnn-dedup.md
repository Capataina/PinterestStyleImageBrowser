---
source_type: paper
date_published: 2025-04
hype_score: 0
---

# Perceptual Hashing vs Deep-Embedding Image Deduplication

## Source reference

- MDPI Electronics 2025 comparative paper: https://www.mdpi.com/2079-9292/15/7/1493
- imagededup (idealo): https://idealo.github.io/imagededup/
- imagehash (Python): https://github.com/JohannesBuchner/imagehash

## Claim summary

Empirical comparison of pHash / dHash / aHash vs CNN-embedding-based image deduplication. **Perceptual hashes show sharp precision drop with rising recall**; CNN embeddings (CLIP-class) maintain high precision across recall range. CNN methods are more robust to crops, rotations, colour shifts.

## Relevance to our project

A4: Substantive backing for using CLIP embeddings (which the project already has) for duplicate detection — superior to perceptual hashing. Adding a "Find Duplicates" feature is one cosine threshold + one DB query: pairs of images with cosine ≥ 0.99 are near-duplicates.

A1: Demonstrates the project's CLIP infrastructure is *already* a better deduplication system than the canonical perceptual-hash path used by tools like Czkawka — the question is exposing it.

## Specific takeaways

- Threshold cosine ≥ 0.99 is the typical near-dup boundary for CLIP-ViT-B/32 (calibrate against the project's actual data).
- Pairs are O(N²) at worst; use the same HNSW recommendation for efficient pair-finding at scale.
- Roadmap M6 explicitly lists "dedup detection" — this paper backs the recommendation.

## Hype indicators

None — peer-reviewed comparative paper.
