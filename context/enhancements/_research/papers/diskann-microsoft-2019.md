---
source_type: paper
date_published: 2019-12
hype_score: 0
---

# DiskANN: Fast Accurate Billion-point Nearest Neighbor Search on a Single Node

## Source reference

- NeurIPS 2019: https://suhasjs.github.io/files/diskann_neurips19.pdf
- Microsoft Research: https://www.microsoft.com/en-us/research/publication/diskann-...
- Authors: Suhas Subramanya et al. (Microsoft Research India)

## Claim summary

Graph-based ANN index (Vamana algorithm) that lives on SSD: indexes a billion-point >100-dim dataset on a single workstation with 64 GB RAM, achieves >95% recall@1 with <5 ms latency.

## Relevance to our project

A4: The project's "scale beyond 100k images" line in Roadmap M6 is exactly DiskANN's territory. For a personal image library, even 100k images is at the *upper end* — DiskANN is over-indexed for the typical use case but cited here because it bounds the upper edge of the relevant scale.

A1: Microsoft has released DiskANN as open source with C++ (and bindings). For Rust the equivalent is `lancedb` (which uses an IVF-PQ + DiskANN-style approach).

## Specific takeaways

- DiskANN is overkill for the project's likely scale; HNSW is the right tool for 1k-100k images.
- Useful as a citation for "the project has a credible scaling path" without requiring DiskANN itself.
- The Vamana algorithm influences modern ANN library design.

## Hype indicators

None.
