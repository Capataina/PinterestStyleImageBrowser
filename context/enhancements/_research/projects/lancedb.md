---
source_type: shipped-project
date_published: 2026-04
hype_score: 2
---

# LanceDB

## Source reference

- GitHub: https://github.com/lancedb/lancedb
- Rust docs: https://docs.rs/lancedb/latest/lancedb/

## Claim summary

Embedded vector database (no server required) written in Rust on top of the Lance columnar format. IVF-PQ index by default. Supports vector search, full-text search, SQL. Multimodal-first design (vectors + metadata + raw images / text columns).

## Relevance to our project

A1 + A4: LanceDB is Rust-native and *embedded* — runs in-process like SQLite. This is the architectural shape that fits the project: replacing the SQLite-with-BLOB-embeddings design with LanceDB would unify storage + index + search into one engine. But this would be **commitment-grade**, not plug-and-play.

A4: For the embedding-quality audit + ANN-index recommendation, LanceDB is the "all-in-one" alternative to "SQLite + HNSW behind a trait".

## Specific takeaways

- IVF-PQ (Inverted-File + Product-Quantisation) is a different ANN index family from HNSW. Different recall/latency characteristics.
- Embedded posture matches Tauri's "no server" stance.
- License: Apache 2.0.
- Last commit: active 2026.

## Hype indicators

Mild — VC-funded but substantive code, growing community.
