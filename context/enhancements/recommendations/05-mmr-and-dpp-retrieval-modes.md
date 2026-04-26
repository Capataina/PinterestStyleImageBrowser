---
audience: ML-infra and retrieval / embedding-systems researchers
secondary_audiences: Rust + applied-ML systems engineers
coupling_grade: plug-and-play
implementation_cost: small (4-6 days)
status: draft
---

# Add MMR and k-DPP retrieval modes; benchmark against the existing 7-tier sampler

## What the addition is

Two additional retrieval-diversification modes alongside the existing three (`get_similar_images` / `_sorted` / `_tiered`):

- **MMR (Maximal Marginal Relevance)** — `get_similar_images_mmr(query, top_n, lambda)`. Iteratively picks images that are relevant to the query but not too similar to images already picked. Single tunable parameter `λ` ∈ [0, 1] trading relevance vs diversity.
- **k-DPP (k-Determinantal Point Process)** — `get_similar_images_kdpp(query, top_n)`. Samples from a DPP whose kernel is the Gram matrix of the candidate embeddings, trading relevance for geometric span. Theoretically principled diversity.

Both implementations are pure-Rust, layered on top of the existing `VectorIndex.search()` (Rec-1). A small benchmark + qualitative comparison file `audits/diversity_comparison.md` shows the visual difference between the four diversity-aware modes (sampled / tiered / MMR / k-DPP) on 10-15 representative queries.

## Audience targeted

**Primary: A4 Retrieval researchers** — `audience.md` Audience 4 signal-function: "Diversity / re-ranking: Documented MMR / DPP / tier-sampler, with a behavioural reason". The four-way comparison is the principled framing this audience expects: heuristic (tier) + classic (MMR) + theoretical (DPP) + baseline (top-K sorted).

**Secondary: A1** — pure-Rust implementations of named algorithms with clear complexity (MMR is O(N·k), k-DPP is O(N·k³ Cholesky)) demonstrate the kind of "implements the textbook" craftsmanship A1 reads.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/papers/mmr-carbonell-1998.md` | MMR is the canonical document/result diversification algorithm. ACM SIGIR foundational, 1000+ citations. |
| 2 | `_research/papers/dpp-kulesza-2012.md` | k-DPP is the principled probabilistic alternative; tractable algorithms, geometric-span interpretation. |
| 3 | `_research/papers/spotify-diversity-recommendation.md` | Spotify research: "high consumption diversity is strongly associated with long-term user metrics." Diversity is a business KPI, not just an aesthetic. |
| 4 | `_research/papers/pinterest-visual-search.md` | Pinterest engineering: "diversity, not just duplicates" is an explicit design objective. Backs the project's existing 7-tier sampler. |
| 5 | `_research/projects/criterion-rs.md` | The benchmark harness for the four-way comparison. |
| 6 | `_research/projects/instant-distance.md` | Both MMR and DPP can run on top of any `VectorIndex` — including the future HNSW from Rec-2. |
| 7 | `_research/papers/ann-benchmarks-aumueller-2018.md` | Recall-vs-diversity is a less-standard axis but the methodology (recall@k + diversity-of-retrieved-set) is well-defined. |
| 8 | `_research/projects/lancedb.md` | Some embedded vector DBs ship MMR / re-rankers built-in; reinforces that this is a recognised primitive. |
| 9 | `_research/projects/qdrant.md` | Qdrant has built-in diversity-aware queries; named industry feature. |
| 10 | `_research/papers/imagebind-meta-2023.md` | Multi-modal retrieval increases the importance of diversity (different modalities have different similarity scales); the four-way comparison generalises. |
| 11 | `_research/notes` (project) — `random-shuffle-as-feature.md` | The project explicitly treats randomness/diversity as a feature; this rec extends that thinking from heuristic to principled. |
| 12 | `_research/notes` (vault) — `Suggestions.md` rec 5 | "Treat the tiered similarity algorithm as load-bearing product design" — the rec explicitly preserves it; just adds named alternatives. |

## Coupling-grade classification

**Plug-and-play** — both algorithms are free functions over `&dyn VectorIndex` + the existing `cosine_similarity` primitive. They do not modify the existing three retrieval modes. They register as additional Tauri commands (`get_similar_images_mmr`, `get_similar_images_kdpp`) wired through `lib.rs` exactly like the existing modes.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, organised around a `CosineIndex` brute-force similarity engine over SQLite-stored f32 BLOBs, with three retrieval modes (sampled / sorted / 7-tier Pinterest).** This rec adds two named diversity-aware modes alongside the existing three. The 7-tier mode stays as the project's default — it remains the "load-bearing product design" per the user's vault Suggestions.md.

```
   Existing retrieval modes (preserved)        New (Rec-5)
   ──────────────────────────────────         ─────────────────
   get_similar_images        (random)         get_similar_images_mmr
   get_similar_images_sorted (top-K)          get_similar_images_kdpp
   get_tiered_similar_images (7-tier)
                                              + audit comparing all 5
                                                on 10-15 fixed queries
```

The frontend gets an optional dropdown (or stays at the default 7-tier) — for 99% of users, nothing visible changes. For the audit + benchmark + portfolio-evidence purpose, the additional modes exist as named, called code paths.

## Anti-thesis

This recommendation would NOT improve the project if:

- The user has settled the diversity question and doesn't care to compare alternatives. The audit's value evaporates without the comparison framing.
- The project's audience is purely A3 (Tauri+React) — they care less about retrieval-diversification theory. But A4 reads this directly, and the cost is small.
- k-DPP becomes computationally too expensive at scale (O(N·k³)). At k=35 (the 7-tier output size) and N=10k, a k³ ≈ 43k operation is fine; at k=100, k³ = 1M — borderline. The recommendation should default to small k.

## Implementation cost

**Small: 4-6 days.**

Milestones:
1. Implement `mmr` as a pure function over `&dyn VectorIndex`: get top-3K candidates from index, then iteratively pick by argmax of `λ·sim(d,q) - (1-λ)·max sim(d,d')`. ~1 day.
2. Implement `kdpp` similarly: get candidates, build Gram matrix (Cholesky), sample via the standard k-DPP sampling algorithm. ~2 days.
3. Wire both as Tauri commands in `lib.rs`. ~½ day.
4. Add 5-8 unit tests (degenerate cases, monotonicity properties, k-DPP determinism with fixed seed). ~1 day.
5. Write `audits/diversity_comparison.md`: pick 10 query images, run all 5 modes, embed the result as a visual diff (or describe the diff if no rendering pipeline). ~1 day.
6. Document the four diversity-aware modes in `context/systems/cosine-similarity.md` (rename to `retrieval-modes.md` if the rename feels right). ~½ day.

Required reading before starting: re-read `Suggestions.md` rec 5 — the project's existing 7-tier sampler is *intentional product design*. The new modes must not displace it; they augment with citable alternatives.
