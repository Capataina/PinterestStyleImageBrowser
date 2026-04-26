---
audience: Rust + applied-ML systems engineers
secondary_audiences: ML-infra and retrieval / embedding-systems researchers
coupling_grade: plug-and-play
implementation_cost: small (3-5 days)
status: draft
---

# Encoder + Index trait abstractions (architectural prerequisite for Recs 2-7)

## What the addition is

Two small Rust traits in `src-tauri/src/similarity_and_semantic_search/`:

```rust
pub trait ImageEncoder: Send + Sync {
    fn embed(&self, img_path: &Path) -> Result<Embedding, EncoderError>;
    fn embed_batch(&self, paths: &[&Path]) -> Result<Vec<Embedding>, EncoderError>;
    fn embedding_dim(&self) -> usize;
    fn name(&self) -> &'static str;  // "openai-clip-vit-b-32", "siglip2-base", etc.
}

pub trait TextEncoder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Embedding, EncoderError>;
    fn embedding_dim(&self) -> usize;
    fn name(&self) -> &'static str;
}

pub trait VectorIndex: Send + Sync {
    fn add(&mut self, key: ImageKey, emb: &Embedding);
    fn search(&self, query: &Embedding, k: usize) -> Vec<(ImageKey, f32)>;
    fn len(&self) -> usize;
    fn name(&self) -> &'static str;  // "brute-cosine", "hnsw-ef200-m16", etc.
}
```

The current `Encoder`, `TextEncoder`, and `CosineIndex` types implement these traits. **Nothing else changes today.** The traits exist to make Recs 2-6 swap-in additions instead of structural rewrites.

## Audience targeted

**Primary: A1 Rust + applied-ML systems engineers** — `audience.md` Audience 1 signal-function highlights "API surface: `Result<T, ApiError>` enums, `tracing` spans, `#[instrument]` boundaries" and "Index design: cache-friendly memory layout, partial-sort over full sort, scratch-buffer reuse, named complexity". A trait-abstracted encoder + index is the canonical Rust expression of "the choice is reversible" — exactly the mental model this cohort applies to portfolio code review.

**Secondary: A4** — for the recall-vs-QPS benchmark recommendation (Rec-2), the `name()` method is what a benchmark suite reads to label results.

## Why it works

| # | Source | What it contributes |
|---|--------|---------------------|
| 1 | `_research/projects/ort-pyke.md` | The current `ort` binding is `2.0.0-rc.X` — API has been stable across rc series; risk of breaking change is real but bounded. A trait isolates that risk. |
| 2 | `_research/projects/burn-tract.md` | Burn and tract are credible alternatives to `ort` if the runtime ever regresses; the trait makes the swap a 1-file change instead of a sweep. |
| 3 | `_research/projects/candle.md` | Candle's WASM story enables a future "encoder-in-WebView" path; the trait keeps that option open. |
| 4 | `_research/projects/instant-distance.md` | The HNSW addition (Rec-2) implements `VectorIndex` cleanly. |
| 5 | `_research/papers/hnsw-malkov-2018.md` | Foundational case for HNSW as the "next index" — trait makes the swap safe. |
| 6 | `_research/papers/siglip2-2025.md` | Encoder-of-the-month (SigLIP-2 → MobileCLIP → DFN-CLIP) is now the norm; trait turns each upgrade into a 1-file swap. |
| 7 | `_research/papers/dinov2-meta-2023.md` | DINOv2 (Rec-4) needs to coexist with CLIP — trait lets two encoders live in the same project. |
| 8 | `_research/firm-hiring/huggingface-rust-eng.md` | HF specifically rewards "system architecture" in Rust JDs; trait abstractions are the evidence shape. |
| 9 | `_research/firm-hiring/anthropic-infra-rust.md` | Anthropic's Sandboxing / Systems engineer roles emphasise "clean, maintainable code"; trait abstractions are the marker. |
| 10 | `_research/projects/qdrant.md` | Qdrant's filterable-HNSW pattern is reusable as a future trait method — trait flexibility now anticipates richer queries later. |
| 11 | `_research/projects/lancedb.md` | Even if LanceDB is rejected as commitment-grade today, the trait keeps it as a *possible* swap if the project ever wants to. |
| 12 | `_research/projects/silentkeys-tauri-ort.md` | Reference architecture — production Tauri+ORT apps use the same trait pattern. |

## Coupling-grade classification

**Plug-and-play.** No behaviour changes. The traits are *introduced* in this rec; concrete `BruteForceCosineIndex` and `OnnxClipEncoder` impls preserve all current behaviour. Subsequent recs add new impls behind the same traits. Zero risk of regressing existing functionality because the existing types *become* the trait impls.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, organised around a `CosineIndex` brute-force similarity engine over SQLite-stored f32 BLOBs.** This recommendation adds three trait declarations and refactors the existing types to implement them. Behaviour is identical.

```
                Before                                After
   ┌──────────────────────────┐           ┌──────────────────────────┐
   │  cosine_similarity.rs    │           │  index.rs                │
   │   ┌────────────────────┐ │           │   trait VectorIndex      │
   │   │ struct CosineIndex │ │           │       │                  │
   │   │   .get_similar_*() │ │           │       │ implementors:    │
   │   └────────────────────┘ │           │       ├ BruteForceCosine │
   └──────────────────────────┘           │       │ (← existing)     │
                                          │       └ HNSWIndex (Rec-2) │
                                          └──────────────────────────┘
   ┌──────────────────────────┐           ┌──────────────────────────┐
   │  encoder.rs              │           │  encoder.rs              │
   │   ┌────────────────────┐ │           │   trait ImageEncoder     │
   │   │ struct Encoder     │ │   ──►     │       │ implementors:    │
   │   │   .encode()        │ │           │       ├ OnnxClipEncoder  │
   │   └────────────────────┘ │           │       │ (← existing)     │
   └──────────────────────────┘           │       └ OnnxSiglipEncoder│
                                          │         (Rec-3)          │
                                          │       └ OnnxDinov2Encoder│
                                          │         (Rec-4)          │
                                          └──────────────────────────┘
```

The `lib.rs` Tauri command handlers continue to pull from `tauri::State<...>` — only the type they pull is the trait object (`Box<dyn VectorIndex>`, `Box<dyn ImageEncoder>`).

## Anti-thesis

This recommendation would NOT improve the project if:

- The user never plans to add a second encoder or index. Then the trait is dead abstraction — though the cost is so small (3-5 days) and the audience-signal (A1, A4) is direct that even unused, it doesn't hurt.
- The traits are over-designed (fancy generics, complex error types) such that the existing impls become harder to read. Discipline matters: keep the traits minimal.
- The user pivots away from A1/A4 audiences entirely (e.g., toward indie-hacker / VC audience). Then the engineering signal is less load-bearing.

## Implementation cost

**Small: 3-5 days.**

Milestones:
1. Define `ImageEncoder`, `TextEncoder`, `VectorIndex` traits in new `src-tauri/src/similarity_and_semantic_search/traits.rs`. ~½ day.
2. Refactor existing `Encoder` → `OnnxClipEncoder`, `TextEncoder` → `OnnxClipTextEncoder`, `CosineIndex` → `BruteForceCosineIndex`, each implementing the corresponding trait. ~2 days.
3. Update `lib.rs` Tauri State to hold `Box<dyn VectorIndex>` etc. ~½ day.
4. Verify all 104+ tests still pass; add 4-6 trait-conformance tests. ~1 day.
5. Document the traits in a new `context/systems/encoder-trait.md` file. ~½ day.

Required research before starting: re-read `context/systems/cosine-similarity.md` and `clip-image-encoder.md` to ensure the trait covers all current method shapes (sampled / sorted / tiered retrieval modes need to be addressable through the trait — likely as `search(...)` returning a sorted Vec, with the diversity / tiered logic moved up a layer).
