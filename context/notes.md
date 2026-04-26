# Notes

Project-level rationale, conventions, and durable lessons. One bullet per note file; full content in the linked file.

## Active work areas

**2026-04-26 Tier 1 + Tier 2 + Phase 4 + Phase 5 + Phase 6 + Phase 7 perf bundle SHIPPED** (commits `f5706ed` → `1761e4e`). Together they break the 22 s `ipc.get_images` freeze chain and add multi-encoder rank fusion as the new primary similarity path. App-support data wiped post-commit so the next launch produces a clean baseline. See `plans/2026-04-26-autonomous-session-report.md` for the full session report.

The encoder pipeline was overhauled on 2026-04-26: CLIP image + text branches swapped to the separate-graph OpenAI English exports (HF tokenizers crate for BPE), DINOv2 upgraded Small (384-d) → Base (768-d) with corrected preprocessing, SigLIP-2 wired in with verified URL + correct exact-square 256×256 + Gemma SentencePiece tokenizer. A comprehensive diagnostic system landed alongside. The embedding-pipeline migration (DB version 3 as of 2026-04-26 with R6+R7+R8) invalidates legacy embeddings on first launch under the new code. Build + 120/120 lib tests + 62/62 vitest pass; clippy clean.

Likeliest near-term landings:

- **R5 FP16 ONNX weights** — DEFERRED in the perf bundle. Needs a 200-image golden test set with hand-labelled known-similar pairs to validate the FP16 vs FP32 recall@10 trade-off before shipping. Until then, FP32 stays the safe default.
- **R10 foreground/background encoder split** — perf-plan Tier 3. Phase 5 RRF changes the calculus: with fusion, the user benefits more from all three encoders running than from one finishing fast. Still valid as a future feature but lower priority.
- **R11 decode-once fan-out** — real perf win, bigger refactor. Worth a focused session.
- **Smart per-query encoder routing** — open architectural concern in `notes/preprocessing-spatial-coverage.md`. Phase 5 RRF mostly obviates this for image-image (fusion automatically blends encoder strengths), but text-side dispatch is still single-encoder per query.
- **Indexing.rs phase-module split** — code-health audit medium. Pure-movement extraction; schedule a hygiene-focused session.
- **`[...slug].tsx` route extraction** — code-health audit medium. Same shape.
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
- [fusion-architecture](notes/fusion-architecture.md) — end-to-end model of the multi-encoder fusion system: the indexing-vs-search loops, why per-encoder enable/disable replaced the picker, why RRF over score-fusion, where settings.json::enabled_encoders lives, lifecycle table, performance shape.
- [encoder-additions-considered](notes/encoder-additions-considered.md) — research-grade inventory of candidate 4th-encoder additions (OpenCLIP-LAION, EVA-CLIP, MobileCLIP, perceptual hashes); decision rule + threshold for when to add one.
