---
source_type: paper
date_published: 2012-07-25
hype_score: 0
---

# Determinantal Point Processes for Machine Learning

## Source reference

- arXiv: https://arxiv.org/abs/1207.6083
- Foundation Trends ML: https://www.nowpublishers.com/article/DownloadSummary/MAL-044
- Authors: Alex Kulesza, Ben Taskar (UPenn)

## Claim summary

DPPs — probabilistic models of repulsion. Diverse subsets are more probable because their feature vectors span larger volumes (geometric interpretation). Tractable inference for marginalisation, sampling, MAP. k-DPP variant fixes the subset size.

## Relevance to our project

A4: The principled alternative to MMR for diversity-aware retrieval. The k-DPP fits the project's "give me 35 images that are both relevant and visually diverse" use case naturally — sample 35 from a DPP whose kernel is the cosine-similarity matrix of the candidate set.

A1: Implementable in pure Rust; the algorithmic core is one Cholesky decomposition + a sampling loop. Roughly 100-200 lines of code over the existing `CosineIndex` primitives.

## Specific takeaways

- A k-DPP retrieval mode would give the project a *theoretically-principled* diversity story to set alongside the heuristic 7-tier sampler.
- Computational cost: O(N·k³) for sampling — fine at the project's 1k-100k scale.
- The kernel matrix could be the gram matrix of CLIP embeddings, restricted to the top-N candidates by query similarity.

## Hype indicators

None — foundational ML monograph.
