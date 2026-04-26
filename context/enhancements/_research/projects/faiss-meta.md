---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# FAISS (Meta / FAIR)

## Source reference

- GitHub: https://github.com/facebookresearch/faiss
- Site: https://faiss.ai/
- Engineering blog: https://engineering.fb.com/2017/03/29/data-infrastructure/faiss-a-library-for-efficient-similarity-search/

## Claim summary

The reference vector-similarity library. C++ + Python wrapper. Implements HNSW, IVF, PQ, OPQ, IVFPQ — the full ANN-index zoo. FAIR-maintained. Used in production at Meta and as the canonical reference in academic comparisons.

## Relevance to our project

A4: FAISS sets the bar for "what is the SOTA ANN library". Any HNSW recommendation should at least mention FAISS as the comparison point. For a Rust project, FAISS itself is a poor fit (C++ FFI), but Rust analogues exist (`usearch`, `instant-distance`, `lancedb` IVF-PQ).

A1: Useful as a credibility anchor: "we benchmarked our HNSW choice against FAISS performance numbers".

## Specific takeaways

- IVF + PQ composite index is FAISS's classic recommendation for >1M vectors.
- For 1k-100k vectors (the project's scale), HNSW is sufficient; no need for the IVF-PQ composite.
- Pinecone uses FAISS-derived techniques internally.

## Hype indicators

None.
