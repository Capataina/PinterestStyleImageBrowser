# Research Plan — Image Browser

Surfaces selected for the four audiences identified in `audience.md`. Targets are **soft targets** (err HIGH); the soft floor for the total is 60, target band 80-120.

## Surface selection

```
                Audience-fit-driven surface selection
   ┌────────────────────────────────────────────────────────────────┐
   │  A1 Rust+ML systems   ──►  GitHub repos · papers · forums       │
   │  A2 Local-first/FHE   ──►  papers · GitHub · funding · talks    │
   │  A3 Tauri+React app   ──►  GitHub · forums · firm-blogs · RFCs  │
   │  A4 Retrieval research──►  papers · GitHub · benchmarks · talks │
   └────────────────────────────────────────────────────────────────┘
```

| Surface (folder slug)       | Target | Floor | Reasoning |
|-----------------------------|:------:|:-----:|-----------|
| `papers/`                   | 18-22  | 14    | A1, A4 (especially) and A2 (FHE) all read primary literature. CLIP / quantisation / ANN / FHE / retrieval-eval all have arXiv-grade canonical work. |
| `projects/` (GitHub repos)  | 22-28  | 18    | A1 + A3 + A4 all triangulate via "what other people shipped". Image-browser / Tauri / vector-DB / ANN-index / FHE-RS / quantised-CLIP comparables exist in volume. |
| `firm-hiring/`              | 6-8    | 4     | Hiring-signal mid-weight: useful but supplemental. Hugging Face / Pinecone / Weaviate / Mozilla / Apple-PCC are the relevant firms. |
| `rfcs-and-issues/`          | 6-8    | 4     | Tauri 2 / `ort` / `tokenizers` / Candle issue threads + RFCs reveal the *direction* of the stack the project sits on. |
| `funding/`                  | 4-6    | 3     | Vector-DB and FHE companies ($-rounds) are an external durability signal; thin but real. |
| `talks/`                    | 6-8    | 4     | RustConf / EuroRust / FHE.org / NeurIPS retrieval talks anchor frontier signal. |
| `forums/` (HN / lobste.rs / Reddit / Substack) | 12-16 | 8 | Zeitgeist signal, hype-flagged heavily. |
| `industry-analyst/`         | 2-4    | 1     | Mostly low-leverage for an OSS project; Gartner-class pieces only useful where they name on-device-AI / privacy-eng as categories. |
| **Total**                   | **76-100** | **56** | Above the 60-floor at floor; comfortably in the 80-120 target band at target. |

## Per-surface execution notes

### `papers/`

Search axes (each yields 3-5 papers):
- **CLIP architecture and reproductions** — Radford et al. 2021 (CLIP), Schuhmann et al. (LAION-5B), open_clip evaluations, EVA-CLIP, SigLIP / SigLIP2 (Zhai 2023, 2025).
- **Approximate-NN indexes** — Malkov & Yashunin 2018 (HNSW), Aumüller et al. (ann-benchmarks), Jégou et al. (PQ), DiskANN (Subramanya 2019), SCANN.
- **Quantisation / on-device CLIP** — TinyCLIP, MobileCLIP (Apple 2024), ONNX-Runtime quantisation guide, INT8 calibration literature.
- **Retrieval diversification** — Carbonell & Goldstein 1998 (MMR), Determinantal Point Processes for retrieval (Kulesza & Taskar 2012), result-set diversification surveys.
- **FHE for vector search** — Cheon-Kim-Kim-Song (CKKS), TFHE (Chillotti et al. 2016+), Apple Wally / Live Caller ID (Apple ML blog 2024-2025), PIR + FHE retrieval papers.
- **Image-retrieval evaluation** — recall@k, mAP, MTEB-like multimodal benchmarks.

Quality bar: peer-reviewed venue OR arXiv preprint with >50 citations OR primary author from named lab. Notes capture: claim, reproducibility, named lab, conference venue.

### `projects/` (GitHub)

Comparables to research:
- **Image-browser apps** — Eagle, Hydrus, Czkawka, ente-photos, XnView, Adobe Bridge alternatives, Lyn, photoview (immich is mid-server but adjacent).
- **Tauri 2 reference apps** — Pot (Tauri translator), Spacedrive (Tauri + Rust file manager), Cap, Arboard, Tauri-app/awesome-tauri.
- **Rust ML inference** — `ort` (pyke), Candle (Hugging Face), Burn, `tract` (Sonos), `wonnx` (web-GPU), `mistral.rs`.
- **Vector indexes in Rust** — `instant-distance`, `hnsw_rs`, `usearch` (has Rust bindings), `qdrant` (server but core-Rust), `lancedb`.
- **FHE in Rust** — `tfhe-rs` (Zama), `concrete` (Zama), `swift-homomorphic-encryption` (cross-ref), OpenFHE (C++ but compared).
- **Tokeniser-in-Rust comparables** — HF `tokenizers`, `bert-tokenizer-rs`, `rust-tokenizers`.

Per-repo data captured: stars, last-commit, contributors, the novel thing they did, what made them respected.

### `firm-hiring/`

Firm careers pages + recent eng-blog posts to capture role-title language:
- Hugging Face (Rust eng), Mozilla (Firefox AI Translate / on-device ML), Apple PCC / Wally team (FHE), Pinecone (vector DB), Weaviate, Qdrant, Anthropic infra, fal.ai, Replicate, Cloudflare Workers AI, Brave (Rust+ML), Modal.

Capture: specific role titles, named requirements ("ONNX runtime", "HNSW", "FHE", "Tauri"), recent eng-blog topics.

### `rfcs-and-issues/`

The "where the underlying stack is heading" surface:
- Tauri 2 RFCs and changelog (CSP, asset-protocol, mobile)
- `ort` issues / discussions (CoreML support, EP fallback semantics, model-format support)
- HF `tokenizers` deviation discussions vs WordPiece/BPE in pure Rust
- ONNX Runtime quantisation + EP issues
- LanceDB / Qdrant retrieval-API design RFCs (informs interface design for our HNSW swap)

### `funding/`

Vector-DB rounds (Pinecone, Weaviate, Chroma, Vespa); FHE startups (Zama, Optalysys, Inpher); on-device-AI (fal.ai, Replicate, Modal). Captures the signal that frontier audiences read as "is this category being invested in?".

### `talks/`

YouTube canon: RustConf, EuroRust, FHE.org Conference, NeurIPS / ICLR poster sessions, Carnegie Mellon Database Group's vector-DB talks, Apple ML lectures.

### `forums/`

HN front-page threads, lobste.rs Rust+ML threads, the `r/rust` and `r/MachineLearning` weekly threads, Tauri Discord recap-threads (via mirrors), Hugging Face forum threads on Rust integration. Hype-flag heavily.

### `industry-analyst/`

Limited leverage — only collected if a Gartner-/IDC-class analyst named "on-device-AI" or "privacy-preserving AI" as a category in 2024-2026.

## Per-recommendation citation budget

Each recommendation file targets 12-15 citations from `_research/`, soft floor 8. Cross-coverage across at least 3 distinct surfaces required per recommendation (for example: 1 paper + 1 GitHub repo + 1 firm hiring page already counts as 3-surface coverage).

## What I will NOT research

| Skipped surface | Why |
|---|---|
| Crunchbase paywall data | Public press coverage of funding rounds is enough; analyst paywalls add no incremental signal for a portfolio project |
| Twitter / X threads | Volatile, deletable, low-substance per call; HN/lobste.rs/Reddit cover the same zeitgeist |
| Patent databases | Not relevant for an OSS project's signal |
| LLM-agent / MCP-tool ecosystems | Trend-chasing audience drift per `audience-identification.md` failure modes; deliberately excluded |

## Risks to flag in the synthesis

- **CLIP itself is being superseded by SigLIP / SigLIP2 / EVA-CLIP / DFN.** This is a project-status signal regardless of any individual recommendation — must be reflected in `synthesis.md` "Worth Working On?".
- **The `ort` crate is on `2.0.0-rc.10`** at the time of writing — durability case for the underlying ML runtime needs explicit examination.
- **Tauri 2 is 18 months old at most.** Not a risk per se, but its long-term posture as a desktop-app stack is part of A3's audience-fit case and should be cross-checked.

## Research execution mode

- **WebSearch + WebFetch** as primary tools.
- Each note is written to `_research/<surface>/<slug>.md` with the structure from `references/research-method.md`.
- Each note's frontmatter records a manual `hype_score` per the heuristic (the script is also available; manual scoring is faster for batch work and the heuristic is identical).
- Surface-coverage tracker maintained inline in `_research/_tracker.md`.
