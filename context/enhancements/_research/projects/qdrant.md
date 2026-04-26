---
source_type: shipped-project
date_published: 2026-03
hype_score: 2
---

# Qdrant

## Source reference

- Site: https://qdrant.tech/
- GitHub: https://github.com/qdrant/qdrant
- v1.16 blog: https://qdrant.tech/blog/qdrant-1.16.x/

## Claim summary

Production-grade vector database written entirely in Rust + SIMD + custom storage engine (Gridstore). Bespoke filterable HNSW that integrates filter conditions into graph traversal (no pre-/post-filtering). v1.16 added ACORN filtering algorithm and inline storage mode.

## Relevance to our project

A1 + A4: The most successful "Rust + vector DB" production project. Validates that the Rust ecosystem is the right choice for vector search at any scale.

A4 specifically: Qdrant's filterable HNSW pattern is exactly the right abstraction for the project's tag-filter-AND-similarity-search interaction. Currently the project's tag filter and cosine similarity are independent paths; a filterable HNSW would let the user say "find similar images that ALSO have tag X" efficiently.

## Specific takeaways

- Qdrant proves Rust is a credible language for production vector DB.
- Funded $40M+ across multiple rounds — strong durability signal.
- The architectural choice (filterable HNSW, payload-aware traversal) is reusable as a *design idea* even if Qdrant itself is too heavy a dependency.

## Hype indicators

Mild — VC-funded startup with marketing voice. Underlying code is real and benchmarked.
