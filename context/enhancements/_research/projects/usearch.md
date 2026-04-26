---
source_type: shipped-project
date_published: 2026-04
hype_score: 2
---

# USearch (Unum)

## Source reference

- GitHub: https://github.com/unum-cloud/usearch
- Rust binding: https://crates.io/crates/usearch
- Benchmarks: https://github.com/unum-cloud/usearch/blob/main/BENCHMARKS.md

## Claim summary

Single-header C++ HNSW implementation with bindings for 11 languages including Rust. Custom-metric support, smaller binary than FAISS. Their own benchmark vs FAISS on 1M-vector workload: USearch 53k vec/s precompiled / 88k vec/s with Numba JIT vs FAISS 77k vec/s.

## Relevance to our project

A1 + A4: Comparable to `instant-distance` and `hnsw_rs` as an HNSW backend. Trade-off: faster on raw HNSW but adds a C++ dependency, which the project has explicitly avoided in `clip-text-encoder` (per Decision D6).

## Specific takeaways

- The Rust binding is non-trivial (FFI to a C++ core); pure-Rust alternatives are preferable for this project's stack.
- Useful as a *comparison point* in the recall-vs-QPS curve, even if not adopted.
- Benchmark suites are well-developed.

## Hype indicators

Mild — Unum markets aggressively but the GitHub repo is real, with code, tests, benchmarks.
