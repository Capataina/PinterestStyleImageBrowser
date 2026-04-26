# Audiences for Image Browser

The audiences below were derived from `README.md`, the LifeOS vault notes (`Projects/Image Browser/Overview.md`, `Architecture.md`, `Decisions.md`, `Suggestions.md`, `Roadmap.md`, `Gaps.md`, `Systems/*`, `Work/Encrypted Vector Search.md`), local `context/architecture.md` and `context/notes/*.md`, the recent git log (27 commits across the perf-diagnostics + multi-folder + CLIP-quality + AND/OR-tag-filter sweep), and `Profile/Professional/Interests.md` for career direction. Each cites verbatim project evidence in Layer 3.

The four chosen audiences are listed in priority order (1 = most directly served by current code; 4 = most aspirational). Recommendations downstream weight 1-3 the heaviest; 4 is partly aspirational and is flagged.

```
                                Audience-fit map
   ┌──────────────────────────────────────────────────────────────────┐
   │                                                                  │
   │   Rust+ML systems (1)        Local-first / privacy-eng (2)       │
   │   ───────────────────        ───────────────────────────         │
   │   ort, ndarray,             Tauri, no-cloud,  TFHE-rs work file, │
   │   pure-Rust tokeniser,      asset-protocol,    "encrypted vector │
   │   on-disk index, CUDA→CPU   single-binary       search" proposed │
   │                                                                  │
   │   Tauri+React app eng (3)    ML-infra / retrieval research (4)   │
   │   ──────────────────────     ────────────────────────────────    │
   │   8 Tauri commands,          7-tier sampler, lazy index,         │
   │   optimistic mutations,      profiling system, partial-sort      │
   │   masonry, perf overlay      finding (1.77× wall-clock)          │
   │                                                                  │
   └──────────────────────────────────────────────────────────────────┘
```

---

## Audience 1 — Rust + applied-ML systems engineers (the primary audience)

### Who they are

| Type | Examples |
|------|----------|
| Firms | Hugging Face (Candle / `tokenizers`), Mozilla (Firefox AI / Translate), Anthropic infra, Modal, fal.ai, Replicate, Cloudflare Workers AI, NVIDIA TensorRT-Rust adopters, Microsoft DirectML / ONNX team, the `ort` maintainers (pyke), `tract` maintainers (Sonos), Fly.io (Rust-shop), Brave (Rust + ML), Element / Matrix (ML in safe Rust) |
| Archetypes | Rust ML inference engineer, Rust systems engineer with ML adjacency, "no-Python ML deployment" practitioner, on-device-AI developer |
| Communities | `r/rust`, `users.rust-lang.org` ML threads, `lobste.rs` Rust+ML threads, the `ort` GitHub Discussions, the Candle / Burn Discord servers, the ML in Rust Slack, the EuroRust + RustConf systems track audience |

### What they value (signal-function)

This cohort rewards portfolio work that **proves ML-without-Python is shippable on commodity hardware** while honouring Rust idioms (no needless `unsafe`, well-typed errors, contained side effects). Signal counts when concrete: numbers, exporters used, kernel choices named, fallback strategies actually tested.

| Dimension | What scores | What doesn't |
|-----------|-------------|--------------|
| Inference correctness | Cosine ≈ 1.0 vs Python reference, deterministic outputs across CPU/CUDA/CoreML, named ONNX export pipeline | "It works locally", screenshots without numbers |
| Tokeniser depth | Pure-Rust WordPiece/BPE, vocab-file driven, multilingual coverage, edge-case test suite | "Wraps `tokenizers` crate" |
| Hardware portability | CUDA + CoreML + CPU all proven to load; per-EP fallback table; documented kernel gaps | "CUDA only", silent CPU fallback |
| Index design | Cache-friendly memory layout, partial-sort over full sort, scratch-buffer reuse, named complexity | `Vec::sort_by` everywhere, no allocation discipline |
| API surface | `Result<T, ApiError>` enums, `tracing` spans, `#[instrument]` boundaries | `unwrap()` in handlers, `println!` for prod logs |

### How the project currently speaks to them

> "Pure-Rust WordPiece tokenizer is genuinely a non-trivial implementation — worth mentioning on a technical skills surface … only project in the vault with that full stack working"
> — `Projects/Image Browser/Suggestions.md` lines 137-139 (LifeOS)

> "Tokenizer is now implemented in pure Rust within encoder_text.rs. No external tokenizer dependency needed!"
> — `Cargo.toml` comment quoted in `Projects/Image Browser/Decisions.md` D6 line 110

> "Two composable optimisations to the three retrieval methods … 1.77× wall-clock speedup at n=10k, top_n=50 in debug … `select_nth_unstable_by` … Reusable scratch buffer … keyed by index into cached_images (NOT cloned PathBufs)."
> — commit `c6551e2` body (HEAD, 2026-04-26)

> "CUDA-first with CPU fallback … `ort` is the most mature Rust ONNX binding"
> — `Decisions.md` D5 lines 86-95

The pure-Rust WordPiece path, the `select_nth_unstable_by` partial-sort, the `(usize, f32)` scratch buffer keyed by index rather than `PathBuf`, and the documented CoreML-disable workaround across both image and text encoders (commits `90a3842`, `2775c9f`) all signal craftsmanship inside the constraint that matters most to this cohort: ML at native speed without Python and without paying allocation tax in the inner loop. The 1.77× number is the kind of citable artefact this audience reads first.

---

## Audience 2 — Local-first / privacy-engineering community

### Who they are

| Type | Examples |
|------|----------|
| Firms | Signal, Apple (Wally / Live Caller ID), Proton, Mullvad, Tutanota, Brave, DuckDuckGo, Obsidian (local-first ethos), Mozilla, Element / Matrix, Tailscale, Anysphere (Cursor — privacy posture) |
| Archetypes | Privacy engineer, on-device-ML advocate, "no-cloud" desktop-app builder, FHE / MPC researcher with eng chops |
| Communities | localfirstweb.dev community, "Ink & Switch" alumni, the Zama / OpenFHE forums, IACR ePrint (FHE / private retrieval threads), HN local-first / privacy threads, the `tfhe-rs` GitHub, `concrete-ml` Discord, `swift-homomorphic-encryption` GitHub watchers |

### What they value (signal-function)

| Dimension | What scores | What doesn't |
|-----------|-------------|--------------|
| Local-by-construction | Single binary, no network in the hot path, no API keys, no telemetry by default | "Optional cloud sync", "you can self-host the inference" |
| Crypto primitives in shipped code | TFHE-rs / BFV / SEAL bindings actually called, encrypted ciphertext-shaped values flowing through the index, named library + version | "We could add FHE later", "the architecture supports it" |
| Threat-model clarity | Adversary named (host process, snooping co-tenant, OS-level reader), assumption table, bound on what the encryption protects vs leaks | Vague "privacy-preserving" framing |
| Honest perf framing | Documented FHE slowdown (orders of magnitude), tractable-envelope spec, named-not-supported retrieval modes | "Privacy without performance cost" hand-waves |
| Comparable artefacts | Cite Apple Wally, Microsoft SEAL ports, Zama Concrete examples | None / single-vendor framing |

### How the project currently speaks to them

> "Privacy by construction: original images are never modified or uploaded; thumbnails and embeddings are derived locally and stored in a local SQLite database … Offline ML inference: CLIP embeddings are generated and compared entirely via ONNX Runtime — no Python, no external ML service, no GPU required"
> — `README.md` lines 65-69

> "Every piece of compute (scanning, DB, thumbnails, CLIP inference) runs on the user's machine. No API keys, no auth, no network required."
> — `Decisions.md` D2 lines 33-45 (LifeOS)

> "Add an encrypted-vector storage path alongside the existing plaintext CLIP embedding storage … TFHE-rs (or BFV) ciphertexts of CLIP embeddings; existing f32 BLOB storage stays … Apple Wally as production-shipped FHE deployment validating the BFV path."
> — `Projects/Image Browser/Work/Encrypted Vector Search.md` lines 20-24 (LifeOS)

> "FHE has a 4-5 orders of magnitude slowdown vs plaintext. The encrypted path is not for 'competing with plaintext speed' — it is for the privacy-sensitive use case where the alternative is 'not running the search at all'."
> — `Work/Encrypted Vector Search.md` line 53

The user has *already* drafted an encrypted-vector path inside the vault; the project is the natural test bed for FHE-on-CLIP (small dataset, identifiable Rust stack, plaintext baseline already shipped). The README's "by construction" framing and the existing TFHE-rs proposal are exactly the kind of artefacts this audience reads as a marker of seriousness rather than a marketing claim.

---

## Audience 3 — Tauri + modern-React desktop-app engineers

### Who they are

| Type | Examples |
|------|----------|
| Firms | Tauri Apps team (Mozilla → CrabNebula), Anysphere (Cursor uses Tauri-adjacent native), Linear (desktop), Notion (local mode), Ente Photos (open-source local-first photos / Flutter but adjacent), Obsidian (local-first), Vercel (React 19 + RSC), Bun (Zig + Rust + JS bridge) |
| Archetypes | "Native-feel desktop with web tech" engineer, React-19 + Rust-IPC fluent dev, performance-aware frontend engineer (server components, masonry, perf overlays) |
| Communities | Tauri Discord and GitHub Discussions, `r/tauri`, `r/rust` "show your project", lobste.rs Tauri threads, React 19 RFC threads, TanStack Query GitHub Discussions, EuroRust desktop track |

### What they value (signal-function)

| Dimension | What scores | What doesn't |
|-----------|-------------|--------------|
| IPC discipline | Typed Tauri command surface, narrow `assetProtocol.scope`, named error enum surviving the boundary | `csp: null`, `scope: ["**"]`, stringified errors |
| Optimistic-UI craft | Cache snapshots, rollback paths, no-spinner mutation flow tested | Naive `await mutate()` then refetch |
| Layout engineering | Custom shortest-column packing, debounced resize, hero-promotion logic | Off-the-shelf Pinterest-clone library |
| App-perf instrumentation | Live overlay + JSONL telemetry + post-exit markdown report | `console.log("slow")` |
| Multi-folder UX | Filesystem watcher, orphan detection, per-root thumbnail cache, AND/OR semantics | Hardcoded scan path |

### How the project currently speaks to them

> "Renders a Pinterest-style masonry grid with shortest-column packing; the currently-selected image is promoted to the top spanning up to 3 columns; each grid item gets a framer-motion 3D tilt on mouse hover"
> — `Projects/Image Browser/Overview.md` line 61

> "Adds the second half of the profiling pipeline … RawEvent enum tagged on `kind`: Span { ts_ms, name, duration_us } … Process-global ringbuffer of pending events, capped at 50,000 … Streamable for the on-exit renderer: it reads line-by-line, never holds the whole timeline in memory at once."
> — commit `26c16e8` body (HEAD~3)

> "Multi-folder support + filesystem watcher + orphan detection"
> — commit `0908550` subject

> "Image annotations + AND/OR tag filter semantics"
> — commit `56990b7` subject

> "Every TanStack Query mutation follows this shape … `onMutate` … cache snapshots … `onError` … rollback. Use this exact pattern for any new mutation."
> — `context/notes/conventions.md` lines 36-57

The recently-shipped profiling system, AND/OR tag semantics, multi-folder + watcher pass, and the optimistic-mutation discipline are all artefacts this audience inspects: the kind of polish that distinguishes a "Tauri demo" from "a Tauri app you would actually use". The custom masonry packer (vs grabbing `react-masonry-css`) is the specific kind of from-scratch detail this cohort notices.

---

## Audience 4 — ML-infra and retrieval / embedding-systems researchers (aspirational anchor)

### Who they are

| Type | Examples |
|------|----------|
| Firms / labs | Meta FAIR (FAISS, ImageBind), Pinecone, Weaviate, Qdrant, Chroma, Vespa, Anthropic retrieval team, OpenAI embeddings team, Cohere embed team, GDM retrieval-augmented systems, Spotify embedding-systems group |
| Archetypes | Vector-DB engineer, retrieval researcher, ANN-index practitioner, embedding-quality auditor |
| Communities | NeurIPS / ICLR retrieval / multimodal sessions, the FAISS GitHub, the `usearch` / `hnswlib` / `instant-distance` discussions, vector-DB benchmark forums (ann-benchmarks), Hugging Face Spaces leaderboards |

### What they value (signal-function)

| Dimension | What scores | What doesn't |
|-----------|-------------|--------------|
| Retrieval mechanism | Named index type (HNSW / IVF / PQ / SCANN), recall vs latency curves, build-vs-query trade-off | "Brute-force cosine" without comparison |
| Quality audit | Cosine-vs-reference Python CLIP, recall@k on a labelled set, ablations across preprocessing | "Looks right" qualitative claims |
| Diversity / re-ranking | Documented MMR / DPP / tier-sampler, with a behavioural reason | "Top-K is enough" |
| Distillation / compression | INT8 / FP16 weights, quantised CLIP, LoRA adapters | None |
| Evaluation discipline | Held-out queries, fixed seeds, reproducible numbers | Anecdote |

### How the project currently speaks to them (partly aspirational)

> "Pinterest-style retrieval — seven tiers (0-5%, 5-10%, ..., 40-50%), 5 random picks per tier = up to 35 results … This is the most product-thoughtful piece of the backend"
> — `Projects/Image Browser/Systems/Cosine Similarity.md` lines 78-103 (LifeOS)

> "Library size >50k images would make full-scan latency noticeable; would swap for `instant-distance` / `hnsw_rs` / similar."
> — `Decisions.md` D8 lines 145-153

> "Embedding quality audit. Sample 50 queries, compare against reference CLIP implementation. Check how much quality was lost to ImageNet-stats + Nearest-filter + truncated-pooling cascades."
> — `Roadmap.md` line 83

> "After swapping, validate by encoding a known image and comparing cosine similarity vs a Python reference — the answer should be ≥ 0.999."
> — `context/notes/clip-preprocessing-decisions.md` line 24

The 7-tier sampler is a defensible diversity-aware retrieval mechanism worth comparing against MMR / DPP — but it has not been benchmarked, no recall@k numbers exist, and there is no comparison harness against a reference Python pipeline. **Aspirational classification:** this audience can be reached IFF the project ships an embedding-quality audit, named ANN-index alternative behind the existing CosineIndex trait, and a small reproducible benchmark. Recommendations downstream propose exactly these. Without those, this audience is "audience the project could speak to with the right additions, but does not currently" — flagged per `audience-identification.md` Layer 3 guidance.

---

## Cross-audience overlap

Several artefacts speak to multiple audiences simultaneously. The recommendation set should weight overlapping wins higher.

| Artefact / direction | A1 Rust+ML | A2 Local-first | A3 Tauri+React | A4 Retrieval |
|---|:--:|:--:|:--:|:--:|
| Embedding-quality audit vs Python CLIP | high | mid | low | high |
| ANN index (HNSW) behind a trait | high | mid | low | high |
| TFHE-rs encrypted-vector path | high | high | low | mid |
| Per-EP (CUDA / CoreML / CPU) bench harness | high | low | mid | mid |
| `tracing` span + JSONL + report polish | high | mid | high | low |
| Typed `ApiError` enum across Tauri IPC | mid | low | high | low |
| INT8 / FP16 quantised CLIP variant | high | mid | low | high |
| Multi-folder + watcher + AND/OR (already shipped) | low | mid | high | low |
| Pure-Rust WordPiece edge-case test pack | high | mid | low | low |
| ann-benchmarks-style reproducible eval | mid | low | low | high |

The high-leverage row is "items rated `high` for at least two audiences": those will dominate Phase 6 priority.

---

## Common-failure self-check

| Failure mode (`audience-identification.md`) | Status |
|---|---|
| Vague archetype | None — every audience names firms / communities |
| Single-audience tunnel vision | Four audiences; the project genuinely speaks to all four |
| Aspirational without anchor | A4 is flagged explicitly; A1-A3 cite verbatim project artefacts |
| Audience-from-domain confusion | Domain is "image-browser app"; chosen audiences are derived from how the project is built (Rust+ML, no-cloud, Tauri, retrieval), not from domain platitudes |
| Trend-chasing audience selection | No agentic / MCP / LLM-orchestration audience added — those would not pass Layer 3 |
| Audience over-specification | All four are cohorts, not individuals |
