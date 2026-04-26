---
audience: ML-infra and retrieval / embedding-systems researchers
secondary_audiences: Rust + applied-ML systems engineers
coupling_grade: plug-and-play
implementation_cost: medium (1-2 weeks)
status: draft
---

# Embedding-quality audit + SigLIP-2 / MobileCLIP encoder swap option

## What the addition is

Two coupled additions:

1. **Embedding-quality audit** — a small Python notebook (`audits/embedding_quality.ipynb`) that loads the project's bundled `test_images/` (or a public proxy if test_images is not committed), encodes them via the *current* OpenAI CLIP-ViT-B/32 ONNX model, then via 3-4 alternative encoders: **SigLIP-2-Base**, **MobileCLIP-S2**, **DFN-CLIP-ViT-B**, and a reference Python OpenAI-CLIP-ViT-B/32 baseline. Reports cosine-vs-baseline (sanity check), zero-shot ImageNet-stats-vs-CLIP-stats sensitivity, and recall@k against a hand-curated set of 20-30 query/relevant-image pairs from the bundled corpus.
2. **A new `OnnxSiglipEncoder: ImageEncoder` impl** behind the Rec-1 trait, so users can switch encoder at startup via the same config mechanism Rec-2 introduced (`--encoder siglip2-base`).

The audit decides which encoder ships as the new default; the trait makes it config-driven so users keep choice.

## Audience targeted

**Primary: A4 Retrieval researchers** — the audit produces a citable artefact: "we ran 4 encoders against the same corpus, here are the numbers". `audience.md` Audience 4 signal-function: "Retrieval mechanism: Named index type, recall vs latency curves" + "Quality audit: Cosine-vs-reference Python CLIP, recall@k on a labelled set, ablations across preprocessing".

**Secondary: A1** — names canonical Rust/ONNX swap path (HF Optimum export → `models/model_image_siglip.onnx` → `OnnxSiglipEncoder`).

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/papers/siglip-zhai-2023.md` | SigLIP outperforms CLIP at small batch sizes; ICCV 2023 peer-reviewed. |
| 2 | `_research/papers/siglip2-2025.md` | SigLIP-2 outperforms SigLIP-1 across all model scales on retrieval. |
| 3 | `_research/projects/siglip2-hf-models.md` | SigLIP-2 ONNX exports available on HF; Apache 2.0; drop-in compatible projection. |
| 4 | `_research/papers/mobileclip-apple-2024.md` | MobileCLIP-S2: 2.3× faster than ViT-B/16 CLIP at higher accuracy; CVPR 2024. |
| 5 | `_research/papers/dfn-clip-apple-2023.md` | DFN-CLIP-ViT-H reaches 84.4% ImageNet zero-shot (above OpenAI WIT). ICLR 2024. |
| 6 | `_research/papers/eva-clip-2023.md` | Additional alternative encoder (BAAI); demonstrates the SOTA frontier is well-populated. |
| 7 | `_research/papers/tinyclip-2023.md` | TinyCLIP: 50% smaller, comparable accuracy. ICCV 2023. Cross-coverage on the "smaller-is-fine" thesis. |
| 8 | `_research/projects/open-clip.md` | The reference Python implementation against which the audit cosine-validates. |
| 9 | `_research/projects/optimum-onnx-export.md` | HF Optimum is the documented pipeline for the ONNX export. |
| 10 | `_research/papers/mteb-evaluation.md` | MTEB-v2 multimodal benchmarks are the publishable comparison reference. |
| 11 | `_research/papers/clip-zero-shot-classification.md` | The technique (text-prompt encoding for zero-shot) used to define recall@k labelled sets. |
| 12 | `_research/papers/onnx-int8-quantization.md` | Couples to Rec-6: any chosen encoder can be INT8-quantised to recover MobileCLIP-class speed. |
| 13 | `_research/firm-hiring/huggingface-rust-eng.md` | HF specifically rewards encoder-aware engineering work in Rust. |
| 14 | `_research/projects/firefox-translate-onnx.md` | Production proof that ONNX-on-Rust is the right runtime for swap-encoder workflows. |
| 15 | `_research/notes` (project) — `clip-preprocessing-decisions.md` | The user's own project notes flag this as "the cheapest known quality win in the codebase" (line 23). |

## Coupling-grade classification

**Plug-and-play** — produces a new ONNX file (`models/model_image_siglip.onnx`) and a new encoder impl behind the Rec-1 trait. Existing OpenAI CLIP path stays. Default encoder selection becomes a config setting whose default the audit informs. If the user disagrees with the audit's recommendation, they switch back via config.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, organised around a `CosineIndex` brute-force similarity engine over SQLite-stored f32 BLOBs.** This recommendation adds (a) an audit notebook documenting encoder-quality differences and (b) a new `ImageEncoder` impl. The existing OpenAI CLIP path stays. The audit's *finding* may motivate the user to flip the default; the *choice* is config-driven and reversible.

```
            Audit pipeline (Python, separate)
   ┌──────────────────────────────────────────┐
   │   audits/embedding_quality.ipynb         │
   │       │                                  │
   │       ├ Load test_images/ corpus         │
   │       ├ Encode via 5 encoders:           │
   │       │   • OpenAI CLIP-B/32 (current)   │
   │       │   • OpenAI CLIP-B/32 (Python ref)│
   │       │   • SigLIP-2-Base                │
   │       │   • MobileCLIP-S2                │
   │       │   • DFN-CLIP-ViT-B               │
   │       │                                  │
   │       ├ Compute pairwise cosines         │
   │       ├ Compute recall@10 on hand-       │
   │       │   curated query set (~25 queries)│
   │       └ Output audits/results.md         │
   └──────────────────────────────────────────┘
                        │
                        │  finding informs default
                        ▼
            Rust runtime (production)
   ┌──────────────────────────────────────────┐
   │   trait ImageEncoder {...}               │
   │       │                                  │
   │       ├ OnnxClipEncoder    (existing)    │
   │       └ OnnxSiglipEncoder  (new)         │
   │                                          │
   │   Library/config.toml:                   │
   │     [encoder]                            │
   │     image_kind = "openai-clip-b32"       │
   │       # or "siglip2-base"                │
   │       # or "mobileclip-s2"               │
   └──────────────────────────────────────────┘
```

**Embedding compatibility:** different encoders produce different embedding spaces. Switching encoders requires re-encoding the corpus. The project already has the `get_images_without_embeddings` predicate (per `Architecture.md`); a future "rescan with new encoder" path is one DB migration: clear the embeddings column, re-run the encoding pipeline. Users opting in get an "encoder swap" UX similar to the existing first-run encoding pass.

## Anti-thesis

This recommendation would NOT improve the project if:

- The user is happy with current similarity quality and doesn't want the storage cost of multiple encoder ONNX files. The audit alone (without the encoder impl) is still the primary signal artefact for A4.
- The audit results show OpenAI CLIP-B/32 actually wins for the project's specific corpus. Possible — small corpora with well-understood semantics sometimes favour the older model. The audit's value is *the answer*, not a predetermined direction.
- A specific newer encoder (e.g., a 2026 release) makes the four candidates obsolete by the time of implementation. Then update the audit; the trait stays valid.

## Implementation cost

**Medium: 1-2 weeks (audit + impl).**

Milestones:
1. Set up the audit Python environment (uv / poetry, `optimum`, `onnxruntime`, `open_clip_torch`). ~½ day.
2. Hand-curate a small query set: 20-30 queries against the bundled corpus, with 3-5 relevant images per query for recall@k. ~1 day (manual curation is the slow step).
3. Encode the corpus through all 5 candidates (Python). ~½ day (mostly waiting).
4. Write the comparison cells: cosine-vs-baseline + recall@k tables + per-encoder size + per-encoder latency. ~1 day.
5. Output `audits/results.md` with a clear winner declaration. ~½ day.
6. Implement `OnnxSiglipEncoder` (or whichever wins) as a new `ImageEncoder` impl. ~2-3 days.
7. Add config setting + startup-time encoder selection. ~1 day.
8. Add a `re-encode` Tauri command + UI affordance for switching encoders. ~2 days.
9. Document the audit + impl in `context/systems/clip-image-encoder.md`. ~½ day.

Required research before starting: re-read `context/notes/clip-preprocessing-decisions.md` — it gives the existing baseline normalisation choice and pre-states the validation gate ("cosine ≥ 0.999 vs Python reference"). The audit must hit that gate against the *current* encoder before declaring any other encoder is better.
