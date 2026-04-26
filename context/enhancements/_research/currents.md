# Currents — Image Browser

Identified across 77 source-notes. Each current is named, classified per `references/research-method.md` §Coupling-grade, and reasoned. Currents ordered by audience-cluster (A1 Rust+ML → A2 Local-first → A3 Tauri+React → A4 Retrieval) then by classification (plug-and-play first).

```
                    Coupling-grade flow
                                    ┌─ PLUG-AND-PLAY  (5)
                                    │
   77 sources ─► 11 currents ───────┼─ COMMITMENT-GRADE (5)
                                    │
                                    └─ PURE HYPE  (1)
```

---

## Current 1 — CLIP-encoder upgrade family (SigLIP-2 / MobileCLIP / EVA-CLIP / DFN-CLIP)

**Sources backing this current:** `papers/siglip-zhai-2023.md`, `papers/siglip2-2025.md`, `papers/mobileclip-apple-2024.md`, `papers/eva-clip-2023.md`, `papers/dfn-clip-apple-2023.md`, `papers/tinyclip-2023.md`, `projects/siglip2-hf-models.md`, `projects/open-clip.md`, `projects/optimum-onnx-export.md`
**Hype score distribution:** median 1, range 0-1
**Classification:** **Plug-and-play**

### What the current is

The OpenAI CLIP-ViT-B/32 (2021) the project ships is being broadly outperformed by a 2023-2025 generation: SigLIP / SigLIP-2 (Google), MobileCLIP / MobileCLIP-2 (Apple), EVA-CLIP / EVA-CLIP-18B (BAAI), DFN-CLIP (Apple), TinyCLIP (Microsoft). The dual-encoder shape (image + text in shared space) is preserved.

### Substantive backing

ICCV 2023 (SigLIP, TinyCLIP), CVPR 2024 (MobileCLIP), ICLR 2024 (DFN), arXiv 2024-2025 (SigLIP-2, EVA-CLIP-18B, MobileCLIP-2). All open-weighted; all ONNX-exportable via HF Optimum.

### Promotional backing

Minimal — primarily blog/HuggingFace explainer posts of the underlying papers.

### Classification reasoning

Plug-and-play because the project's existing `Encoder` and `TextEncoder` types in `encoder.rs` / `encoder_text.rs` already abstract over an ONNX session. Swapping the model file (and possibly the projection dim) is a *config* change, not a structural one — provided an "encoder-behind-trait" abstraction is added (recommendation Rec-1).

### Audience relevance

A1 (Rust+ML systems): "we benchmarked CLIP-B/32 / SigLIP-B / MobileCLIP-S2 on our local corpus, here are the numbers" is exactly the kind of benchmark-first-claim artefact this audience reads.
A4 (Retrieval): the dominant alternative encoders are real and quality-superior; comparing them against a baseline is publishable-quality work.

---

## Current 2 — Approximate-NN indexes (HNSW, IVF-PQ, ScaNN, DiskANN)

**Sources backing this current:** `papers/hnsw-malkov-2018.md`, `papers/diskann-microsoft-2019.md`, `papers/ann-benchmarks-aumueller-2018.md`, `papers/scann-google-2020.md`, `projects/instant-distance.md`, `projects/usearch.md`, `projects/qdrant.md`, `projects/lancedb.md`, `projects/faiss-meta.md`, `forums/brute-vs-hnsw-small.md`, `funding/vector-db-funding-2024.md`
**Hype score distribution:** median 0, range 0-2
**Classification:** **Plug-and-play**

### What the current is

The vector-search field has converged on a small set of canonical ANN indexes: HNSW (most popular for in-memory), IVF-PQ (more memory-efficient at billion-scale), ScaNN (Google's), DiskANN (SSD-resident at billion-scale). For a desktop-app-scale library (1k-100k images), HNSW is the right shape; in Rust the canonical choices are `instant-distance` and `hnsw_rs`.

### Substantive backing

IEEE TPAMI (HNSW), NeurIPS 2019 (DiskANN), Information Systems 2019 (ann-benchmarks), ICML 2020 (ScaNN). Production at Pinecone, Weaviate, Qdrant, Milvus. All have well-defined recall/QPS curves.

### Promotional backing

Vendor blogs from each VectorDB are mildly promotional but technically substantive.

### Classification reasoning

Plug-and-play because the project's `CosineIndex::get_*similar*` methods can be re-expressed against an HNSW backend behind a trait, with the brute-force version retained as a "small dataset" or "exact recall" fallback. The 7-tier sampler pattern can be implemented over either backend.

### Audience relevance

A4 (Retrieval): The only credible answer to "how does this scale beyond 50k images". Recommendation Rec-2 captures the trait + HNSW addition.

---

## Current 3 — Retrieval diversification (MMR, k-DPP, tier-sampling)

**Sources backing this current:** `papers/mmr-carbonell-1998.md`, `papers/dpp-kulesza-2012.md`, `papers/spotify-diversity-recommendation.md`, `papers/pinterest-visual-search.md`
**Hype score distribution:** median 0, range 0-1
**Classification:** **Plug-and-play**

### What the current is

A canonical research thread on retrieval diversification: MMR (1998, query-vs-redundancy lambda), k-DPP (2012, geometric span maximisation), tier-sampling (project's own choice). All three are sound; all three give different visual results. The "compare them" exercise is well-defined.

### Substantive backing

ACM SIGIR 1998 (MMR, 1000+ citations), Foundations & Trends in ML (DPP), Spotify research (diversity vs accuracy), Pinterest engineering blog (production).

### Classification reasoning

Plug-and-play — adding additional retrieval modes alongside the existing three (`get_similar_images`, `get_similar_images_sorted`, `get_tiered_similar_images`) is purely additive code. ~150 LOC per new mode.

### Audience relevance

A4: A "diversity-aware retrieval comparison" benchmark is the kind of citable artefact this audience reads. Backs the project's existing 7-tier sampler (already a defensible product choice) by setting it alongside named alternatives.

---

## Current 4 — On-device-AI / privacy-engineering (Apple PCC, Wally, TFHE-rs, Concrete-ML)

**Sources backing this current:** `papers/wally-apple-2024.md`, `papers/ckks-inner-product-2024.md`, `papers/pacmann-panther-2024.md`, `projects/tfhe-rs-zama.md`, `projects/swift-homomorphic-encryption.md`, `projects/zama-concrete-ml.md`, `firm-hiring/apple-pcc.md`
**Hype score distribution:** median 1, range 0-2
**Classification:** **Commitment-grade**

### What the current is

FHE-on-vector-search is now a *real* category — Apple ships Wally in production iOS 18, Pacmann/Panther are top-tier-venue 2024 papers, TFHE-rs and Concrete-ML are mature open-source FHE libraries. The slowdown is real (4-5 OOM vs plaintext) but the algorithmic envelope has narrowed enough for practical (small-scale) deployment.

### Substantive backing

ICLR 2025 (Pacmann), CCS 2025 (Panther), Apple ML Research (Wally), Apple's open-sourced swift-homomorphic-encryption, Zama's $73M-funded TFHE-rs/Concrete-ML stack.

### Promotional backing

Zama's marketing voice is moderate; the underlying code is real.

### Classification reasoning

**Commitment-grade**: adopting FHE for the project's vector path involves significant new code surface (encrypted index, encrypted similarity primitive, two-process architecture for FHE-tractable retrieval) AND introduces honest performance trade-offs that change the user experience. The user has *already* drafted this as a Work file (`Encrypted Vector Search.md`) with explicit "additive on top of plaintext path" framing — preserving the plaintext default keeps the commitment-grade aspect bounded to the FHE feature alone.

The durability case is strong: multiple top-tier venue papers, Apple production deployment, well-funded library maintainer (Zama), named research labs (NYU, MIT, CMU).

### Audience relevance

A2 (Local-first / privacy): The most direct audience-fit signal in the entire research corpus. The user's existing vault Work file confirms intent. Recommendation Rec-7 captures this.

---

## Current 5 — Tauri 2 + Rust + React desktop-app pattern as a credibility category

**Sources backing this current:** `projects/tauri2-stable.md`, `projects/awesome-tauri.md`, `projects/spacedrive.md`, `projects/silentkeys-tauri-ort.md`, `forums/tauri-vs-electron-2025.md`, `forums/react-19-best-practices.md`, `projects/tanstack-query-optimistic.md`, `forums/framer-motion-perf.md`, `rfcs-and-issues/tauri-dialog-folder-picker.md`, `rfcs-and-issues/tauri-asset-protocol-csp.md`
**Hype score distribution:** median 1, range 0-4
**Classification:** **Plug-and-play** (the underlying decisions; specific React-19 hooks and Motion v12 patterns are individually plug-and-play)

### What the current is

Tauri 2 went stable Oct 2024. Multiple production apps now ship the Rust + Tauri + JS-frontend stack (Spacedrive 30k stars, SilentKeys, Voxly, Watson.ai, Hoppscotch). Bundle sizes 8-15 MB vs Electron 100+ MB; memory 30-40 MB vs 200-300 MB. React 19 (compiler, useOptimistic, concurrent rendering) layers cleanly. Motion v12 (formerly framer-motion) refines animation perf at scale.

### Substantive backing

Tauri's official site + GitHub releases, the awesome-tauri curated list, multiple independent perf benchmark posts that triangulate to consistent numbers, React.dev 19 release notes.

### Promotional backing

The "Tauri vs Electron" benchmark blogs are SEO-shaped; the benchmark *numbers* converge across them.

### Classification reasoning

The stack is already in place — recommendations are about specific incremental hardenings (CSP scope, asset-protocol scope, React-19-aware patterns, Motion v12 patterns, virtualization at 100+ items). Each hardening is plug-and-play.

### Audience relevance

A3 (Tauri+React): All recommendations layered onto an already-shipping stack. The credibility marker is "we ship a real Tauri 2 app with the perf overlay + multi-folder + AND/OR tag semantics".

---

## Current 6 — DINOv2 / image-only embeddings as a *separate* axis from CLIP

**Sources backing this current:** `papers/dinov2-meta-2023.md`, `forums/clip-vs-dinov2-similarity.md`, `papers/imagebind-meta-2023.md`, `papers/simclr-chen-2020.md`
**Hype score distribution:** median 1, range 0-2
**Classification:** **Plug-and-play**

### What the current is

DINOv2 (Meta, 2023) outperforms CLIP **on image-only similarity tasks** by significant margins (5× on fine-grained species, 64% vs 28% on a challenging dataset per multiple independent benchmarks). CLIP retains the advantage for *text-conditioned* retrieval (semantic search). DINOv3 (2024) further improves.

### Substantive backing

CVPR / ICLR / arXiv-grade primary research from Meta + multiple independent reproductions.

### Classification reasoning

Plug-and-play in the same way Current 1 is plug-and-play: another encoder behind the same trait. The difference is *which retrieval path uses which encoder* — DINOv2 for "View Similar", CLIP/SigLIP for "semantic search". Two encoders co-resident is a small additional storage cost.

### Audience relevance

A4: The dual-encoder pattern (DINOv2 + CLIP) is the modern best practice. Recommendation Rec-4 captures it.

---

## Current 7 — Quantisation (INT8 / FP16) for on-device CLIP

**Sources backing this current:** `papers/onnx-int8-quantization.md`, `papers/tinyclip-2023.md`, `papers/mobileclip-apple-2024.md`, `projects/optimum-onnx-export.md`
**Hype score distribution:** median 1, range 0-1
**Classification:** **Plug-and-play**

### What the current is

INT8 / FP16 quantisation of vision encoders is the standard pre-production hardening: 2-4× CPU speedup, cosine ≥ 0.99 vs FP32 with proper static calibration. ONNX Runtime supports it natively; HF Optimum produces the quantised file.

### Classification reasoning

Plug-and-play: produces a `model_image_int8.onnx` file, swapped in via config. Falls back to FP32 trivially.

### Audience relevance

A1 + A4: The standard "we made this fast enough on CPU" artefact. Backs the local-first stance.

---

## Current 8 — Production observability via tracing + OTLP

**Sources backing this current:** `projects/tokio-tracing.md`, `projects/firefox-translate-onnx.md`
**Hype score distribution:** median 0, range 0-1
**Classification:** **Plug-and-play**

### What the current is

`tracing` + `tracing-subscriber` + `tracing-opentelemetry` is the canonical Rust observability stack. The project already migrated to `tracing` (commit `7918e39`) and built a custom Subscriber Layer for the perf-diagnostics overlay. Adding an OTLP exporter completes the production-grade observability story.

### Classification reasoning

Plug-and-play: the existing tracing infrastructure stays; the OTLP exporter is opt-in via config flag.

### Audience relevance

A1 + A3: Demonstrates production-grade engineering posture.

---

## Current 9 — Auto-tagging via CLIP zero-shot + perceptual-quality dedup

**Sources backing this current:** `papers/clip-zero-shot-classification.md`, `papers/perceptual-hash-vs-cnn-dedup.md`, `projects/czkawka.md`, `projects/photoprism-self-hosted.md`, `projects/hydrus.md`, `projects/eagle-app.md`, `projects/immich.md`
**Hype score distribution:** median 1, range 0-3
**Classification:** **Plug-and-play**

### What the current is

The "automatically tag photos + find duplicates" feature pair is what every comparable in the category provides (PhotoPrism, Immich, Hydrus, Eagle). Auto-tagging is CLIP zero-shot classification against a fixed label dictionary; dedup is cosine ≥ 0.99 over the existing CLIP index.

### Classification reasoning

Plug-and-play: the project's CLIP infrastructure already produces the right primitives. Auto-tagging is one Tauri command + a label dictionary; dedup is one query against the existing CosineIndex.

### Audience relevance

A3: Closes the "what does Hydrus / PhotoPrism do that I don't" gap.
A4: Demonstrates how to derive multiple features from a unified embedding space (the Pinterest-style "unified visual embeddings" pattern).

---

## Current 10 — Embedded vector DB shape (LanceDB / Qdrant) as the all-in-one alternative to SQLite-with-BLOBs

**Sources backing this current:** `projects/lancedb.md`, `projects/qdrant.md`, `papers/onnx-int8-quantization.md`
**Hype score distribution:** median 2, range 1-2
**Classification:** **Commitment-grade — REJECTED for this project**

### What the current is

LanceDB and similar embedded vector DBs unify storage + index + search into one engine. For a project starting from scratch this is the cleaner architecture; for this project, switching from SQLite-with-BLOBs to LanceDB is a significant restructure.

### Classification reasoning

Commitment-grade — would replace the entire `db.rs` plus the `CosineIndex` plus the BLOB-cast machinery with a different abstraction. The durability case is moderate (LanceDB is well-funded, $30M Series A) but **identity / direction concerns dominate**: the project's "one SQLite file holds everything" design is a defensible architectural decision per Decision D3/D4. Replacing it would not strictly dominate on every dimension (LanceDB has its own learning curve, dependencies, format-stability questions).

**This current is recorded but no recommendation is drafted from it.** The recommended HNSW addition (Rec-2) lives *behind a trait* alongside the existing CosineIndex — a much smaller commitment that achieves the same scaling outcome.

### Audience relevance

Mentioned in the Field Landscape but does not produce a recommendation.

---

## Current 11 — "MCP / agent / autonomous LLM tool ecosystem" wrapper layer

**Sources backing this current:** None in the corpus (deliberate)
**Hype score distribution:** N/A
**Classification:** **PURE HYPE — REJECTED**

### What the current is (would-be)

A trend in 2024-2026: hype around wrapping local tools as MCP servers / LLM-agent-callable tools. The fashion suggests turning the project's `semantic_search` into an "MCP tool an agent can invoke".

### Classification reasoning

The skill's `audience.md` deliberately did NOT identify "LLM-agent ecosystem" as an audience — the project has zero existing signal for it (no tool wrappers, no agent integrations, no MCP-related code). Recommending an MCP wrapper would be **trend-chasing audience selection** per the Common-failure self-check in `audience.md`.

A wrapper is *technically possible* but the value would be zero for the project's actual audiences (A1-A4). If the LLM-agent fashion fades — and many similar fashions have faded fast — the wrapper becomes dead code.

### Audience relevance

None to the project's identified audiences. Explicitly rejected.

---

## Currents not pursued (recorded for honesty)

- **Stable Diffusion / generative AI add-ons** — the `projects/candle-stable-diffusion.md` note is collected but no recommendation drafted; would be a domain shift (image-browse → image-generate).
- **Mobile (iOS / Android) port via Tauri 2 mobile** — possible per `projects/tauri2-stable.md` but not aligned with audience signal-functions; A3 specifically values the desktop-app discipline.
- **Patent / commercial licensing** — recorded in `industry-analyst/oss-license-2025.md` but the recommendation is just "pick MIT or Apache 2.0 explicitly".

---

## Coupling-grade summary table

| Current | # Sources | Hype median | Classification | → Recommendation? |
|---------|:---------:|:-----------:|----------------|---|
| 1. CLIP-encoder upgrade family | 9 | 1 | Plug-and-play | Rec-3 |
| 2. ANN indexes (HNSW etc.) | 11 | 0 | Plug-and-play | Rec-2 |
| 3. Retrieval diversification | 4 | 0 | Plug-and-play | Rec-5 |
| 4. On-device privacy / FHE | 7 | 1 | Commitment | Rec-7 |
| 5. Tauri 2 + Rust + React stack | 10 | 1 | Plug-and-play | Rec-8 / Rec-9 |
| 6. DINOv2 / dual-encoder | 4 | 1 | Plug-and-play | Rec-4 |
| 7. Quantisation INT8 | 4 | 1 | Plug-and-play | Rec-6 |
| 8. tracing + OTLP observability | 2 | 0 | Plug-and-play | Rec-10 |
| 9. Auto-tagging + dedup | 7 | 1 | Plug-and-play | Rec-11 |
| 10. Embedded vector DB swap | 3 | 2 | Commitment | **No** (out of scope) |
| 11. MCP / agent wrapper | 0 | n/a | Pure hype | **Rejected** |

Plus one cross-cutting recommendation (Rec-1, "encoder behind trait + ANN behind trait + observability hooks") that is the architectural prerequisite for Rec-2/3/4/6/10 — making the project's ML / retrieval surface a clean swap-in/swap-out abstraction. This is the highest-leverage single addition because it unlocks all the others.
