---
source_type: shipped-project
date_published: 2025-12
hype_score: 0
---

# instant-distance

## Source reference

- GitHub: https://github.com/instant-labs/instant-distance
- Crates.io: https://lib.rs/crates/instant-distance

## Claim summary

Pure-Rust HNSW implementation. Powers Instant Domain Search's word-vector indexing in production. Minimal API surface; binary serialisation of indexes via serde.

## Relevance to our project

A1 + A4: The most idiomatic-Rust HNSW. ~1k stars. Direct replacement for the project's brute-force `CosineIndex`. The trait surface is minimal so the existing tiered-sampler logic can call into it without major refactor.

## Specific takeaways

- Fully pure-Rust, no C deps — preserves the project's "no C in the hot path" stance.
- Serializable index format — enables persistent on-disk caching (the project already has a "persistent cosine cache" per recent commit `5fdecf2`; an HNSW index file is a natural extension).
- Less feature-rich than `hnsw_rs` but smaller surface = lower risk of breaking changes.
- Last commit: active in 2025.

## Hype indicators

None — substantial OSS project with real production user.
