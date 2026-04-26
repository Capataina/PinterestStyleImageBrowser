---
source_type: paper
date_published: 2020-08
hype_score: 0
---

# ScaNN — Scalable Nearest Neighbors (Google)

## Source reference

- ICML 2020 paper: https://arxiv.org/abs/2106.00125 (Anisotropic Vector Quantization, the underlying technique)
- GitHub: https://github.com/google-research/google-research/tree/master/scann
- Zilliz Learn: https://zilliz.com/learn/what-is-scann-scalable-nearest-neighbors-google

## Claim summary

Google's high-performance ANN library. Tree-based clustering + anisotropic vector quantisation for fast distance computation. Used in Google Photos, YouTube. Strong on x86 with AVX. The third major ANN family alongside FAISS/HNSW.

## Relevance to our project

A4: Reference for "what does Google use for visual similarity at scale". For the project, ScaNN is a *theoretical* alternative to HNSW — but ScaNN is C++ + TensorFlow-tied, so the practical Rust path remains HNSW.

## Specific takeaways

- ScaNN is ANN-benchmarks-grade competitive with HNSW at the recall/QPS Pareto front.
- No first-class Rust binding exists; the Rust path stays with `instant-distance` / `hnsw_rs`.
- Useful as a citation when discussing the ANN landscape — shows familiarity with the broader space.

## Hype indicators

None — Google research with code.
