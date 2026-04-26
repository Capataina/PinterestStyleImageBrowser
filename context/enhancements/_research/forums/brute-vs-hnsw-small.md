---
source_type: forum-blog
date_published: 2026-01
hype_score: 1
---

# Brute-Force vs HNSW — Small Datasets (10k vectors)

## Source reference

- Zilliz: https://zilliz.com/ai-faq/how-does-the-choice-of-index-type-eg-flat-bruteforce-vs-hnsw-vs-ivf-influence-the-distribution-of-query-latencies-experienced
- Qdrant: https://qdrant.tech/course/essentials/day-2/what-is-hnsw/
- Milvus: https://milvus.io/ai-quick-reference/how-does-the-choice-of-index-type-eg-flat-bruteforce-vs-hnsw-vs-ivf-influence-the-distribution-of-query-latencies-experienced

## Claim summary

Industry consensus: at ~10k vectors, brute-force is *fine* and gives **deterministic, predictable latency + exact recall@k**. HNSW pulls ahead at higher scale (>50k typically). Qdrant's default `full_scan_threshold` is 10000 — confirming the rule of thumb. Below 10k vectors, there is no need for HNSW.

## Relevance to our project

A4: Important honest signal for the HNSW recommendation downstream. The project today is at 1k-2k images per scan; Decision D8 says "Library size >50k images would make full-scan latency noticeable; would swap for `instant-distance` / `hnsw_rs` / similar."

The recommendation should NOT be "swap to HNSW now" — it should be "abstract the index behind a trait so HNSW can be swapped in *when* the user crosses the 10k-50k threshold". This is a more defensible additive recommendation.

## Specific takeaways

- The current brute-force CosineIndex is correct for the project's scale.
- The improvement is the *trait* (allowing future swap), not the *swap itself*.
- When the user does cross 50k images, `instant-distance` or `hnsw_rs` are the right Rust libraries.

## Hype indicators

Mild (vector-DB vendor blogs have promotional voice but the technical claim aligns across vendors).
