---
audience: ML-infra and retrieval / embedding-systems researchers
secondary_audiences: Rust + applied-ML systems engineers
coupling_grade: plug-and-play
implementation_cost: medium (1 week)
status: draft
---

# DINOv2 image-only encoder for "View Similar"; CLIP retained for semantic search

## What the addition is

A second encoder running alongside CLIP/SigLIP: **DINOv2-Small (21M params)** — image-only, no text alignment. The "View Similar" path (image-clicked → tiered similar images) routes through DINOv2; the semantic-search path (typed query → similar images) continues to use CLIP/SigLIP. Two embedding columns coexist in the SQLite schema (`embedding_clip` + `embedding_dinov2`), each populated by its own encoder pass.

## Audience targeted

**Primary: A4 ML-infra and retrieval / embedding-systems researchers** — `audience.md` Audience 4 signal-function names "Retrieval mechanism: Named index type", and DINOv2 is the canonical image-only image retrieval choice as of 2024-2025. Multiple independent benchmarks show DINOv2 dominates CLIP for image-image similarity — the dual-encoder architecture is the documented best practice.

**Secondary: A1** — demonstrates the encoder-trait pattern (Rec-1) generalising to multiple co-resident encoders.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/papers/dinov2-meta-2023.md` | DINOv2 outperforms OpenCLIP on *image-only* benchmarks at every scale; Meta CVPR-track work. |
| 2 | `_research/forums/clip-vs-dinov2-similarity.md` | Multiple independent benchmark reports: DINOv2 64% vs CLIP 28% on challenging dataset; 70% vs 15% on fine-grained 10k-class species (5× advantage). |
| 3 | `_research/papers/simclr-chen-2020.md` | The contrastive-learning lineage; DINOv2 inherits and improves. ICML 2020 foundational. |
| 4 | `_research/papers/imagebind-meta-2023.md` | The Meta multimodal lineage; reinforces the "different encoders for different tasks" pattern. |
| 5 | `_research/projects/open-clip.md` | Reference Python pipeline for both CLIP and DINOv2 weight loading + ONNX export via `optimum`. |
| 6 | `_research/projects/optimum-onnx-export.md` | DINOv2 has ONNX exports via the standard `optimum` toolkit. |
| 7 | `_research/papers/pinterest-visual-search.md` | Pinterest's "unified visual embeddings" pattern is one approach; the dual-encoder pattern is its named alternative. |
| 8 | `_research/papers/perceptual-hash-vs-cnn-dedup.md` | Demonstrates CNN/transformer features dominate handcrafted hashes for visual similarity — DINOv2 is the SOTA in that family. |
| 9 | `_research/papers/spotify-diversity-recommendation.md` | Different similarity functions for different query types is canonical recommendation-systems practice. |
| 10 | `_research/projects/instant-distance.md` | Each encoder gets its own HNSW index trivially under the Rec-1 / Rec-2 traits. |
| 11 | `_research/projects/qdrant.md` | Production vector DBs routinely host multiple embedding spaces per collection; pattern is mature. |
| 12 | `_research/projects/lancedb.md` | Cross-coverage on multi-embedding storage at the embedded-DB layer. |

## Coupling-grade classification

**Plug-and-play** — sits behind the Rec-1 `ImageEncoder` trait. The CLIP path is preserved unchanged. The DINOv2 path is a new column + a new encoder + a routing decision in `lib.rs::get_tiered_similar_images` ("if config flag is set, query against `dinov2_index`, else against `clip_index`"). Removing the rec deletes the new column, drops the impl, restores the routing. No data is destroyed.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, organised around a `CosineIndex` brute-force similarity engine over SQLite-stored f32 BLOBs.** This recommendation adds a second image encoder (DINOv2) with its own SQLite column and its own `VectorIndex` instance. The two encoders coexist; each query path picks the appropriate encoder.

```
                  ┌─────────────────────────────┐
                  │   Tauri command surface     │
                  │                             │
   semantic_search │       get_tiered_similar   │
      (query=text) │       (query=image_id)     │
          │        │              │             │
          ▼        │              ▼             │
   ┌──────────────┐│   ┌──────────────────┐    │
   │ ClipText     ││   │ if config.dual:  │    │
   │   Encoder    ││   │   Dinov2Encoder  │    │
   └──────┬───────┘│   │ else:            │    │
          │        │   │   ClipEncoder    │    │
          ▼        │   └────────┬─────────┘    │
   ┌──────────────┐│            │              │
   │ ClipImage    ││            ▼              │
   │   VectorIndex│   ┌──────────────────┐    │
   │   (existing) │   │  Dinov2Vector    │    │
   └──────────────┘│   │   Index (new)    │    │
                  │   └──────────────────┘    │
                  └────────────────────────────┘
```

The schema migration: add `embedding_dinov2 BLOB` column. Existing `embedding` (CLIP) column unchanged. The encoding pipeline runs both encoders during initial scan (or only one, if the user opts out via config).

## Anti-thesis

This recommendation would NOT improve the project if:

- The user prefers a single unified embedding space for code simplicity (Pinterest's "unified visual embeddings" pattern). Then stay with CLIP/SigLIP only — the project's existing direction supports that.
- Storage cost is a concern at large scales. Two 512-d embeddings per image is 4 KB per image — at 100k images, 400 MB. Probably fine for personal libraries; flag for libraries past 1M.
- DINOv2's encoding latency is meaningfully higher than CLIP and the user is highly latency-sensitive. The 21M-param DINOv2-Small variant is comparable to CLIP-ViT-B/32; larger DINOv2 variants are more expensive.

## Implementation cost

**Medium: 1 week.**

Milestones:
1. Export DINOv2-Small to ONNX via `optimum`. ~½ day.
2. Implement `OnnxDinov2Encoder: ImageEncoder` (largely paralleling `OnnxClipEncoder` — same preprocessing pipeline, different mean/std, different output). ~2 days.
3. Schema migration: add `embedding_dinov2` column via the existing `ALTER TABLE` migration pattern in `db.rs`. ~½ day.
4. Update `Encoder::encode_all_images_in_database` to populate both columns (or just one, per config). ~1 day.
5. Add the dual-encoder routing in `lib.rs` Tauri commands. ~1 day.
6. Run an inline `audits/dual_encoder_compare.md` — 10 image queries through both indices, manual visual comparison. ~½ day.
7. Document the dual-encoder choice in `context/systems/clip-image-encoder.md` (rename to `image-encoders.md`). ~½ day.

Required reading before starting: the audit from Rec-3 should run first to validate the encoder-swap infrastructure. DINOv2 then layers naturally on top.
