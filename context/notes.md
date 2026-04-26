# Notes

Project-level rationale, conventions, and durable lessons. One bullet per note file; full content in the linked file.

## Active work areas

**Latest perf bundle landed 2026-04-26 (commits `f5706ed` → `1ca42d2`):** Tier 1 + Tier 2 (chain-breaking the 22 s `ipc.get_images` freeze + thumbnail/encoder speedups), Phase 4 (SigLIP-2 text dispatch), Phase 5 (multi-encoder RRF for image-image), Phase 6 (code-health bundle + clippy gate restored), Phase 7 (1 Hz RSS/CPU sampler + stall analysis), Phase 11 (per-encoder enable/disable toggles + parallel encoders + text-image RRF), Phase 12 (perf bundle from perf-1777226449: dynamic intra_threads, sequence-thumb-then-encode, SigLIP-2 text pre-warm, fast_image_resize for encoder preprocess, empty-state regression fix). Pipeline version = 4. Build green: 125/125 cargo lib · 62/62 vitest · clippy clean.

The encoder pipeline overhaul context: CLIP image + text on separate-graph OpenAI English exports (HF `tokenizers` BPE), DINOv2-Base (768-d) with canonical preprocessing, SigLIP-2 Base 256 with Gemma SentencePiece. All three encoders run concurrently during indexing, with intra_threads tuned to share the M2 P-cluster (4 threads total across N enabled encoders). Multi-encoder RRF fusion is the primary search path for both image-image and text-image; per-encoder toggles control which encoders contribute.

Future-work items that haven't shipped yet, ordered by likely next-pickup:

- **Code-health audit findings** — 28 findings landed at `plans/code-health-audit/` (highest-impact: legacy single-encoder commands D-SIM-1/D-SEM-1/D-FE-1 are dead post-Phase-11d and ~600 Rust + ~80 TS lines could be deleted; `Settings::priority_image_encoder` is doc-deprecated but still read in `indexing.rs`; `db::get_embedding` skips the read-only secondary connection — a 2-line fix that closes the last R2 gap).
- **R5 FP16 ONNX weights** — Xenova/onnx-community publish FP16 variants of all three encoders; ~1.5-2× CPU speedup, half the disk size. Deferred because the FP16 exports use `float16` I/O (not just FP16 weights), requiring `half::f16` boundary conversion AND a labelled retrieval-quality test set. Without that golden set, swapping risks silent retrieval-quality regression.
- **R10 foreground/background encoder split** — was originally to make one encoder finish fast for early interactivity. Phase 5 RRF + Phase 12c parallel encoders mostly obviate this; still valid for fresh-folder UX but lower priority.
- **R11 decode-once fan-out** — JPEG currently decoded 2-4× per image (thumbnail + each encoder). Sharing the decoded buffer would save ~50-150 ms/image. Bigger refactor; worth a focused session.
- **R12 `with_disable_per_session_threads`** — needs verification that the `ort 2.0-rc.10` Rust binding exposes it.
- **R13 `deadpool-sqlite` connection pool** — cleaner than the current manual writer/reader split. Probably unneeded post-R2 unless concurrent-read requirements grow.
- **Profiling diagnostics expansion** — Phase 4 + 5 shipped; Phase 1 (causal trace substrate with `span_id`/`parent_id`), Phase 2 (deeper DB decomposition beyond the `get_images` subspans), Phase 3 (frontend invalidation tracing), Phase 6 (perfdump/perfdiff CLIs + scenario runner) remain proposed. Lower urgency post Tier-1+2+11+12.
- **Tier 4 research bets** — INT8 quantisation (R14), MobileCLIP-S2 evaluation (R15), one last CoreML attempt with `MLProgram + RequireStaticInputShapes` (R16). All require benchmarking commitment before shipping.
- **Smart per-query encoder routing** — open architectural concern in `notes/preprocessing-spatial-coverage.md`. Mostly obviated for image-image by RRF; text-side dispatch is still single-fusion-pass per query.
- **Indexing.rs phase-module split** — pure-movement extraction; code-health audit recommends a 4-file split into pipeline/encoder_phase/etc. Schedule for a hygiene-focused session.
- **`[...slug].tsx` route extraction** — same hygiene category. Pulls route-state hooks out of the 516-line component.
- **Watcher rebuild on root mutations** — `add_root` / `remove_root` after launch don't reconfigure the watcher until next restart. Documented in `systems/watcher.md`.
- **Path normalisation at insert time** — closes the second half of `notes/path-and-state-coupling.md`.

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
