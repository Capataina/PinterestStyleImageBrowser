# Notes

Project-level rationale, conventions, and durable lessons. One bullet per note file; full content in the linked file.

## Active work areas

The encoder pipeline was overhauled on 2026-04-26: CLIP image + text branches swapped to the separate-graph OpenAI English exports (HF tokenizers crate for BPE), DINOv2 upgraded Small (384-d) → Base (768-d) with corrected preprocessing, SigLIP-2 wired in with verified URL + correct exact-square 256×256 + Gemma SentencePiece tokenizer. A comprehensive diagnostic system landed alongside (12 named diagnostics covering startup state, embedding quality, search rankings, tokenization, preprocessing samples, encoder run summaries, cross-encoder comparisons, and cosine math sanity). The embedding-pipeline migration (DB version 2) invalidates legacy embeddings on first launch under the new code so next indexing pass re-encodes cleanly. Build + 105/105 lib tests pass; not yet committed.

Likeliest near-term landings:

- **Wire SigLIP-2 text encoder through the picker** — the encoder is fully implemented; what's missing is `commands::semantic::semantic_search` reading the user's `textEncoder` preference and dispatching accordingly. Today the picker accepts the choice but the runtime always uses CLIP. See `systems/siglip2-encoder.md` § Partial / In Progress.
- **Smart per-query encoder routing** — open architectural concern in `notes/preprocessing-spatial-coverage.md`. Color/scenery queries belong in SigLIP-2 (no crop, full-image coverage); character/object queries belong in CLIP. Decision deferred.
- **Pipeline parallelism (#74)** — overlap thumbnails + encoding via independent worker threads using the DB as a queue. Tracked in `plans/pipeline-parallelism-and-stats-ui.md`.
- **Pipeline stats UI (#75)** — surface `db::get_pipeline_stats` (already implemented backend-side) in the Settings drawer or status pill. Same plan file.
- **Watcher rebuild on root mutations** — today's gap (`add_root` / `remove_root` after launch don't reconfigure the watcher until next restart). Documented in `systems/watcher.md`.
- **Path normalisation at insert time** — would close the second half of `notes/path-and-state-coupling.md`.

## Index

- [local-first-philosophy](notes/local-first-philosophy.md) — every byte stays on the user's machine; the only network call is first-launch model download from HuggingFace.
- [clip-preprocessing-decisions](notes/clip-preprocessing-decisions.md) — history of CLIP preprocessing: previous Nearest + ImageNet-stats shortcut now replaced by canonical bicubic + CLIP-native; embedding-pipeline migration handles invalidation; spatial-coverage concern remains open.
- [preprocessing-spatial-coverage](notes/preprocessing-spatial-coverage.md) — open architectural concern: CLIP/DINOv2 center-crop drops edge content (problematic for splash arts / scenery / color queries); SigLIP-2 sees the full image; possible direction is smart per-query encoder routing.
- [conventions](notes/conventions.md) — tracing instrumentation prefixes, Mutex acquire-then-execute, `?`-via-From-impls for ApiError, optimistic mutation pattern, `paths::*_dir()` as the single disk-path source, submodule layout, RAII guards for atomics, defensive `lock_result.is_ok()` in setup, naming, `record_diagnostic` pattern.
- [path-and-state-coupling](notes/path-and-state-coupling.md) — the audit closed the cosine-DB-coupling half (now `&ImageDatabase`) and extracted `paths::strip_windows_extended_prefix`; normalise-at-insert is still the deeper fix.
- [random-shuffle-as-feature](notes/random-shuffle-as-feature.md) — Phase 9 made stable-by-default the new behaviour; in-cosine diversity sampling and tiered within-tier randomness remain intentional.
- [dead-code-inventory](notes/dead-code-inventory.md) — Phase 2 sweep + Phase 6 wiring + audit extractions closed the bulk of the previous list; residual is small (3 backend, 1 frontend, 3 deps).
- [mutex-poisoning](notes/mutex-poisoning.md) — five long-lived sync primitives now (DB, cosine Arc, text encoder, watcher slot, indexing AtomicBool); typed-error migration surfaces poisoning as `ApiError::Cosine` instead of opaque strings; `parking_lot::Mutex` is still the strict-upgrade if it bites.
