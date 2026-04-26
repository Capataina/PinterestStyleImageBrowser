---
source_type: paper
date_published: 2018-07-15
hype_score: 0
---

# ANN-Benchmarks: A Benchmarking Tool for Approximate Nearest Neighbor Algorithms

## Source reference

- arXiv: https://arxiv.org/abs/1807.05614
- GitHub: https://github.com/erikbern/ann-benchmarks
- Live results: https://ann-benchmarks.com/
- Authors: M. Aumüller, E. Bernhardsson, A. Faithfull
- Information Systems 2019

## Claim summary

Standardised benchmark for in-memory ANN libraries: standard datasets (SIFT, GloVe, Fashion-MNIST, etc.), standard recall/QPS measurement, Docker containers per algorithm, automatic plot generation.

## Relevance to our project

A4: This is the canonical evaluation harness for any ANN-index recommendation. If the project replaces brute-force CosineIndex with HNSW (or an HNSW alternative), benchmarking against ann-benchmarks-shaped queries (or, better, a project-specific small benchmark using the same recall/QPS axes) gives the exact citable artefact this audience reads.

A1: A benchmark harness in Rust against a fixed test corpus (e.g., the project's `test_images/` if regenerated, or a public CLIP-512 benchmark like the SIFT1M-formatted one) is a high-signal addition.

## Specific takeaways

- The recall-QPS Pareto front is the standard plot.
- Comparing 3-4 indexes (brute / HNSW / IVF-PQ / `usearch`) with one shared dataset is enough to publish-quality.

## Hype indicators

None — well-established research tool.
