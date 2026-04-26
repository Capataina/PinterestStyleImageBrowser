---
source_type: paper
date_published: 2018-12
hype_score: 0
---

# Efficient and Robust Approximate Nearest Neighbor Search Using Hierarchical Navigable Small World Graphs (HNSW)

## Source reference

- arXiv: https://arxiv.org/abs/1603.09320
- IEEE TPAMI: https://dl.acm.org/doi/10.1109/TPAMI.2018.2889473
- Authors: Yu. A. Malkov, D. A. Yashunin

## Claim summary

Multi-layer proximity graph for approximate nearest-neighbour search. Logarithmic search complexity. The single most-cited and most-shipped ANN index in production vector databases (Qdrant, Weaviate, Milvus, Pinecone all use HNSW or HNSW-derivatives). Reference benchmarks report 66× speedup at 90% recall vs brute-force.

## Relevance to our project

A4 (Retrieval) + A1 (Rust+ML): The project's `CosineIndex` is brute-force linear scan. At ~10k images this is fine (<10ms per query). At 50k+ it becomes the bottleneck and the project's own Decisions.md D8 anticipates an HNSW swap. Doing it now (behind the existing `populate_from_db / get_*similar*` interface) is a straightforward additive recommendation.

A1 specifically: Rust HNSW implementations (`hnsw_rs`, `instant-distance`, `usearch`) are mature and pure-Rust — no new C dependency, fits the project's local-first stack.

## Specific takeaways

- HNSW is parameterised by `M` (graph degree) and `efSearch` (search beam). Recall and latency scale with both.
- Disk-backed variants exist (DiskANN below), but for a desktop app at 1k-100k scale, in-memory HNSW is the right shape.
- The 7-tier Pinterest sampler can be implemented over an HNSW index by querying with a larger `efSearch`, partitioning by score, and sampling within tiers — preserving the project's product-thoughtful retrieval semantics.

## Hype indicators

None — foundational paper, IEEE-published, ubiquitous in production.
