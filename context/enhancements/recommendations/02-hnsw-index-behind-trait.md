---
audience: ML-infra and retrieval / embedding-systems researchers
secondary_audiences: Rust + applied-ML systems engineers
coupling_grade: plug-and-play
implementation_cost: medium (1-2 weeks)
status: draft
---

# HNSW index implementation behind the `VectorIndex` trait + recall/QPS benchmark

## What the addition is

A second `VectorIndex` implementation: `HNSWVectorIndex`, backed by the pure-Rust `instant-distance` crate. The brute-force `BruteForceCosineIndex` (Rec-1) stays as the default; HNSW becomes available via a startup config flag (`--index hnsw` or `[index] kind = "hnsw"` in a config file).

A `benches/index_compare.rs` Criterion suite produces a recall-vs-QPS Pareto curve comparing the two backends across N=1k, 10k, 50k, 100k synthetic 512-d vectors plus the real `test_images/` dataset. Output: `bench-results/index_pareto.md` with a table and an inline ASCII chart.

## Audience targeted

**Primary: A4 ML-infra and retrieval / embedding-systems researchers** — `audience.md` Audience 4 signal-function highlights "Retrieval mechanism: Named index type (HNSW / IVF / PQ / SCANN), recall vs latency curves, build-vs-query trade-off" and "Quality audit: Cosine-vs-reference Python CLIP, recall@k on a labelled set". The recall-vs-QPS Pareto curve is the canonical artefact this audience reads first.

**Secondary: A1 Rust + applied-ML systems engineers** — names a concrete Rust-native ANN library (`instant-distance`) and demonstrates trait-based extension, both of which A1 specifically rewards.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/papers/hnsw-malkov-2018.md` | HNSW: O(log N) search complexity, 66× speedup at 90% recall vs brute. |
| 2 | `_research/projects/instant-distance.md` | Pure-Rust HNSW, in production at Instant Domain Search. Idiomatic crate. |
| 3 | `_research/papers/ann-benchmarks-aumueller-2018.md` | Recall-vs-QPS is the *canonical* ANN benchmark axis; standardised methodology. |
| 4 | `_research/forums/brute-vs-hnsw-small.md` | Industry consensus: at <10k vectors, brute-force is fine; >50k, HNSW wins. The trait makes the *threshold-aware* default possible. |
| 5 | `_research/projects/usearch.md` | Cross-coverage: another Rust HNSW (with C++ FFI) — Rust-native (`instant-distance`) is the cleaner choice. |
| 6 | `_research/projects/qdrant.md` | Production proof that HNSW is the workhorse of modern vector search; bespoke Rust HNSW is feasible. |
| 7 | `_research/projects/faiss-meta.md` | FAIR's reference comparison point — the project should be benchmarkable against FAISS-published numbers. |
| 8 | `_research/papers/diskann-microsoft-2019.md` | Bounds the upper edge: at billion-scale, DiskANN; at the project's scale, HNSW is the right tool. |
| 9 | `_research/funding/vector-db-funding-2024.md` | $750M Pinecone, $30M LanceDB — vector indexes are a funded growing category. Substantive durability signal. |
| 10 | `_research/projects/criterion-rs.md` | The Rust-native benchmark harness that produces publish-quality numbers. |
| 11 | `_research/projects/lancedb.md` | Validates the alternative path (IVF-PQ); the trait keeps the option open without committing. |
| 12 | `_research/projects/notify-rs.md` | The project's filesystem-watcher already triggers re-indexing; HNSW.add() is incremental, fits the watcher pattern. |
| 13 | `_research/papers/scann-google-2020.md` | Google's ScaNN as a third reference family — used in Google Photos for the same primitive. |

## Coupling-grade classification

**Plug-and-play** — sits behind the `VectorIndex` trait from Rec-1. The brute-force impl is unchanged and remains the default. HNSW is opt-in via a single config / CLI flag. If HNSW becomes problematic, removing the impl + a single line of config restores the project to the pre-Rec-2 state.

```
                    ┌─ default ─► BruteForceCosineIndex (existing)
   --index <kind> ──┤
                    └─ opt-in ──► HNSWVectorIndex      (new)
```

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, organised around a `CosineIndex` brute-force similarity engine over SQLite-stored f32 BLOBs.** Rec-1 introduced the `VectorIndex` trait. Rec-2 adds a second implementation using `instant-distance`. The brute-force impl stays as the default and the only one used unless the user opts in.

```
   Today (after Rec-1):                    After Rec-2:
   ┌──────────────────────┐               ┌──────────────────────┐
   │  trait VectorIndex   │               │  trait VectorIndex   │
   │      │               │               │      │               │
   │      └ BruteForce    │               │      ├ BruteForce    │ ← still default
   │        CosineIndex   │               │      │  CosineIndex  │
   └──────────────────────┘               │      │               │
                                          │      └ HNSWVector    │ ← new
                                          │        Index         │   opt-in
                                          └──────────────────────┘
                                                         │
                                                         ▼
                                                 instant-distance crate
                                                         │
                                                         ▼
                                                 Persistent index file
                                                 (next to images.db)
```

The 7-tier Pinterest sampler logic moves up a layer (it was inside `cosine_similarity.rs`; it becomes a free function `fn tiered_sample(idx: &dyn VectorIndex, query: &Embedding) -> Vec<...>` that calls `idx.search(query, large_k)` and partitions). This preserves the project's product-thoughtful retrieval semantic (Decision D7) regardless of which index is used.

The benchmark suite is independent: `cargo bench --bench index_compare` runs Criterion against a synthetic dataset + the project's own corpus, produces a markdown table, optionally graphs.

## Anti-thesis

This recommendation would NOT improve the project if:

- The user firmly intends the project to stay below 10k images forever. Brute-force is genuinely sufficient at that scale; HNSW adds dependency surface (`instant-distance`) that buys nothing.
- The project pivots toward server-mode (e.g., becomes a cloud-sync product). Then a server-side vector DB (Qdrant) becomes the right answer rather than an embedded HNSW.
- The Rec-1 trait abstraction was not done. Then HNSW becomes a structural surgery, not a plug-in — and the cost-benefit shifts.

The recommendation assumes the user keeps the project on its current trajectory of "make it actually usable on real personal libraries", which means scaling past the 10k boundary at some point.

## Implementation cost

**Medium: 1-2 weeks.**

Milestones:
1. Add `instant-distance` to `Cargo.toml`; spike a 100-line proof-of-concept HNSW over CLIP embeddings. ~1 day.
2. Implement `HNSWVectorIndex: VectorIndex`. Preserve the `(usize → ImageKey)` mapping that the existing brute-force impl uses internally. ~3 days.
3. Add config plumbing: CLI flag `--index hnsw` + env var override + simple TOML config in `Library/config.toml` (which already exists per recent commits). ~1 day.
4. Add persistent on-disk caching for the HNSW index (write-through on `add`; load on startup). The project already has a "persistent cosine cache" (commit `5fdecf2`); follow the same pattern. ~2 days.
5. Move the 7-tier sampler logic to a free function operating over `&dyn VectorIndex`. Keep the existing tests passing. ~1 day.
6. Write `benches/index_compare.rs` Criterion suite. Generate synthetic 512-d vectors at N=1k/10k/50k/100k; measure recall@10, recall@50, p50/p95/p99 latency for both indexes. ~2 days.
7. Produce `bench-results/index_pareto.md` with the comparison table + a small inline ASCII chart of the Pareto front. ~½ day.
8. Update `context/systems/cosine-similarity.md` to document the dual-index choice + measured numbers. ~½ day.

Required reading before starting: `context/notes/random-shuffle-as-feature.md` to understand which retrieval modes need exact ranking vs which can use approximate.
