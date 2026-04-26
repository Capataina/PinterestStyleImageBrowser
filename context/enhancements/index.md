# Image Browser — Enhancement Recommendations Index

11 surviving recommendations from the 2026-04-26 first-run pass. Ordered by audience-cluster (A1 → A2 → A3 → A4), cheap-first within each cluster. Total surviving recommendations: 11. Total Phase-7 rejections: 0.

```
                  Audience-cluster ordering
   ┌──────────────────────────────────────────────────────────────┐
   │  A1 Rust+ML systems     A2 Local-first/privacy                │
   │   • Rec-1 traits          • Rec-7 encrypted vector            │
   │   • Rec-6 quantisation                                        │
   │   • Rec-10 OTLP                                               │
   │                                                                │
   │  A3 Tauri+React app     A4 Retrieval research                 │
   │   • Rec-8 CSP/scope       • Rec-2 HNSW                        │
   │   • Rec-9 typed errors    • Rec-3 encoder audit               │
   │                           • Rec-4 DINOv2 dual                 │
   │                           • Rec-5 MMR + k-DPP                 │
   │                           • Rec-11 auto-tag + dedup           │
   └──────────────────────────────────────────────────────────────┘
```

## Recommendations Table

| # | Title | Primary Audience | Coupling | Cost | One-line summary |
|---|-------|------------------|----------|------|------------------|
| **01** | [Encoder + Index trait abstractions](recommendations/01-encoder-and-index-traits.md) | A1 Rust+ML | Plug-and-play | 3-5 days (small) | Architectural prerequisite — three small traits behind which existing types implement; unlocks Recs 2/3/4/6/7. |
| **06** | [INT8 quantisation + per-EP benchmark matrix](recommendations/06-int8-quantisation-encoders.md) | A1 Rust+ML | Plug-and-play | 3-5 days (small) | Static-INT8 encoder variant (2-4× CPU speedup) + a published per-encoder × per-precision × per-EP latency matrix. |
| **10** | [OpenTelemetry OTLP exporter for tracing](recommendations/10-tracing-otlp-export.md) | A1 Rust+ML | Plug-and-play | 2-3 days (small) | Optional second tracing-subscriber Layer that exports the existing PerfLayer spans via OTLP. |
| **07** | [Encrypted vector search MVP (TFHE-rs)](recommendations/07-encrypted-vector-search-mvp.md) | A2 Local-first | Commitment | 8-12 weeks (large) | Third VectorIndex impl backed by TFHE-rs ciphertexts; plaintext path stays default; honest perf framing. |
| **08** | [CSP + dynamic asset-protocol scope hardening](recommendations/08-tauri-csp-asset-scope-hardening.md) | A3 Tauri+React | Plug-and-play | 2-3 days (small) | Replace `csp: null` + `scope: ["**"]` with restrictive CSP + dynamic scope tracking user folder roots. |
| **09** | [Typed `ApiError` enum + `parking_lot::Mutex`](recommendations/09-typed-error-enum-and-mutex-replacement.md) | A3 Tauri+React | Plug-and-play | 2-4 days (small) | Discriminated-union errors across the IPC boundary; non-poisoning Mutex for the three singletons. |
| **02** | [HNSW index implementation behind the trait + benchmark](recommendations/02-hnsw-index-behind-trait.md) | A4 Retrieval | Plug-and-play | 1-2 weeks (medium) | `instant-distance` HNSW as opt-in second VectorIndex; recall-vs-QPS Pareto benchmark. |
| **03** | [Embedding-quality audit + SigLIP-2 encoder swap](recommendations/03-clip-encoder-upgrade-audit.md) | A4 Retrieval | Plug-and-play | 1-2 weeks (medium) | Python audit comparing 4-5 encoders on the project's corpus; Rust impl for the winner behind the trait. |
| **04** | [DINOv2 image-only encoder for "View Similar"](recommendations/04-dinov2-image-only-encoder.md) | A4 Retrieval | Plug-and-play | 1 week (medium) | Dual-encoder pattern: DINOv2 for image-image similarity, CLIP for text-image semantic search. |
| **05** | [MMR + k-DPP retrieval modes](recommendations/05-mmr-and-dpp-retrieval-modes.md) | A4 Retrieval | Plug-and-play | 4-6 days (small) | Two principled diversity-aware retrieval modes alongside the existing 7-tier sampler; comparison audit. |
| **11** | [Auto-tagging via CLIP zero-shot + Find Duplicates](recommendations/11-auto-tagging-and-dedup.md) | A4 Retrieval | Plug-and-play | 1-2 weeks (medium) | Two derived features over existing CLIP infra; closes feature-parity gap with PhotoPrism / Hydrus / Eagle / Immich. |

## Cost summary

| Cost band | Count | Total estimated effort |
|-----------|:-----:|------------------------|
| Small (≤1 week) | 6 | 17-26 days |
| Medium (1-2 weeks) | 4 | 4-8 weeks |
| Large (8+ weeks) | 1 | 8-12 weeks |

If only one recommendation is acted on: **Rec-1 (encoder + index trait abstractions)**. It's 3-5 days, unlocks 5 of the other 10 recs as small additive changes, and is itself a portfolio-readable artefact.

## Cross-link map

```
                    Rec-01 (traits)
                         │
                         ├──── Rec-02 (HNSW impl)
                         ├──── Rec-03 (encoder audit + new impl)
                         ├──── Rec-04 (DINOv2 dual-encoder)
                         ├──── Rec-06 (INT8 variant)
                         └──── Rec-07 (EncryptedCosineIndex impl)

   Rec-05 (MMR/DPP) ── reads any VectorIndex (uses Rec-1)

   Rec-08 (CSP/scope) ── independent (Tauri shell hardening)
   Rec-09 (typed errors / parking_lot) ── independent (Rust hardening)
   Rec-10 (OTLP) ── independent (observability)

   Rec-11 (auto-tag + dedup) ── reads existing CLIP infra + tag system;
                                 benefits from Rec-2 (HNSW for find_duplicates)
```

## Recommended sequencing

Three sensible orderings depending on intent:

### Sequence A — "Polish what's there" (optimises portfolio signal at lowest cost)

1. Rec-9 (typed errors + parking_lot) — 2-4 days
2. Rec-8 (CSP + scope) — 2-3 days
3. Rec-1 (traits) — 3-5 days
4. Rec-6 (INT8 + EP matrix) — 3-5 days
5. Rec-10 (OTLP) — 2-3 days
6. Rec-5 (MMR + k-DPP) — 4-6 days

**Total: ~3-4 weeks. Result: a much more A1/A3-credible project with five named additions, all cheap, all preserving the project's identity.**

### Sequence B — "Demonstrate retrieval depth" (optimises A4 signal)

1. Rec-1 (traits) — prerequisite
2. Rec-3 (encoder audit + SigLIP-2 swap) — 1-2 weeks
3. Rec-2 (HNSW + benchmark) — 1-2 weeks
4. Rec-4 (DINOv2 dual) — 1 week
5. Rec-5 (MMR + k-DPP) — 4-6 days
6. Rec-11 (auto-tag + dedup) — 1-2 weeks

**Total: ~6-8 weeks. Result: a project that competes on retrieval research depth — citable benchmarks across encoders, indexes, and diversification modes.**

### Sequence C — "Deep privacy commitment" (optimises A2 signal; assumes FHE appetite)

1. Rec-9 + Rec-8 (foundation hygiene) — 1 week
2. Rec-1 (traits) — 3-5 days
3. Rec-7 (encrypted vector search MVP) — 8-12 weeks

**Total: ~10-14 weeks. Result: a project with a real on-device FHE story — directly references Apple Wally, cites Pacmann/Panther papers, ships a working encrypted-search path with honest perf framing. Strong A2 signal but high cost.**

## Mutual-exclusivity / conflict notes

- None. Every rec is additive on top of the others. Recs 2/3/4/6/7 share Rec-1 as a prerequisite but otherwise compose freely.
- Rec-3 (encoder audit) might inform whether Rec-4 (DINOv2 dual) is also needed or whether a single best encoder is sufficient — running 3 before 4 is recommended.
- Rec-7 (encrypted search) is significantly larger than the rest; treat it as a separable arc, not a sprint task.

## What's NOT in this set (rejected at Phase 7 — none)

Zero rejections in this run. A would-be 12th recommendation was deliberately not drafted from the "Embedded vector DB swap" current (LanceDB / Qdrant) because the commitment-grade restructure does not strictly dominate the existing SQLite+CosineIndex design — see `_research/currents.md` Current 10. A would-be 13th from the "MCP / agent wrapper" hype current was rejected as pure hype with zero substantive sources — see Current 11.

See `synthesis.md` for the full executive report.
