---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# criterion.rs — Statistics-Driven Microbenchmarking

## Source reference

- GitHub: https://github.com/bheisler/criterion.rs
- Docs: https://bheisler.github.io/criterion.rs/book/

## Claim summary

Canonical Rust microbenchmarking library, port of Haskell's Criterion. Statistical analysis detects performance changes between runs. Generates gnuplot charts of distributions. Detects regressions automatically.

## Relevance to our project

A1 + A4: For the embedding-quality audit + per-encoder benchmark recommendation, criterion.rs is the right harness. The project's existing diagnostic test for partial-sort speedup (`tests/cosine_topk_partial_sort_diagnostic.rs`) already pins one number; a criterion-based suite would publish a full distribution.

## Specific takeaways

- Criterion produces statistically rigorous benchmarks that survive PR review at canonical Rust crates.
- For benchmarks comparing CosineIndex (brute) vs HNSW (instant-distance / hnsw_rs), criterion is the standard tool.
- Adds zero runtime cost — benchmarks live under `benches/` and only run via `cargo bench`.

## Hype indicators

None — utility crate.
