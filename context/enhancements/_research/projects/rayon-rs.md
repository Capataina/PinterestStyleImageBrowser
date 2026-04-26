---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# rayon — Rust Data Parallelism

## Source reference

- GitHub: https://github.com/rayon-rs/rayon
- Docs: https://docs.rs/rayon

## Claim summary

Canonical Rust data-parallelism library. Work-stealing scheduler. `par_iter()` converts sequential iterators into parallel ones. Guarantees data-race freedom via Rust's borrow checker. Lightweight overhead; suitable for any task large enough to amortise.

## Relevance to our project

A1: The project's `Roadmap.md` line 65 ("Parallel thumbnail generation") explicitly names rayon as the right tool. Recent commit `5fdecf2` already shipped "parallel thumbnails". Adding rayon to the encoding pipeline (currently serial inside batches) is the next obvious additive recommendation.

## Specific takeaways

- For thumbnail generation (independent CPU-bound JPEG encode/decode), rayon delivers ~4-8× on multi-core.
- For CLIP encoding, the bottleneck is the ONNX runtime call itself (already internally batched); rayon adds value at the *outer* loop (parallel batch dispatch).
- Already used in the project per recent commits — well-understood by the user.

## Hype indicators

None.
