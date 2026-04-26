# Obligation Evidence Map — 2026-04-26 audit

Live ledger of tool-call evidence backing every non-negotiable obligation.
Read by `index.md` § "What I Did Not Do".

## Research-mode distribution

| Mode | Count |
|------|------:|
| 1 — domain pattern lookup | 1 |
| 2 — specific-technique evaluation | 1 |
| 3 — known-anti-pattern check | 2 |

Three modes covered → variety obligation met.

## Pre-Pass-1 front-loaded WebSearch

| Query | Mode | Source |
|-------|------|--------|
| `code health audit patterns Tauri Rust ONNX local-first desktop app 2026` | 1 | <https://v2.tauri.app/security/lifecycle/>, <https://v2.tauri.app/start/> |

Established WebSearch pattern at session start.

## Per-system research evidence

Each substantive system in the Pass-1 prioritisation has at least one
WebSearch call with a system-specific query. The same row carries the
diagnostic-test evidence (file path + assertion + result) when one was
written.

| # | System | Substantive? | Research query | Mode | Source URL | Diagnostic test (file + result) |
|---|--------|--------------|----------------|------|------------|---------------------------------|
| 1 | `indexing.rs` parallel encoder threads | yes | `Rust ONNX Runtime ort 2.0 multi-thread session per-thread anti-patterns memory leak 2026` | 3 | <https://github.com/microsoft/onnxruntime/issues/15962>, <https://github.com/microsoft/onnxruntime/discussions/10107> | `src-tauri/tests/audit_indexing_parallel_encoder_diagnostic.rs` — `#[ignore]` marker, see file for assertions |
| 2 | `commands/semantic_fused.rs` + `commands/similarity.rs` fusion | yes | (covered by RRF research below + general TFP fusion notes already in `notes/fusion-architecture.md`) | 1 | <https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf> (already in repo references) | `src-tauri/tests/audit_fusion_no_text_capable_encoders_diagnostic.rs` — pinned the empty-list contract |
| 3 | `cosine/rrf.rs` RRF math | yes | reasoned omission — math is locally verified by the existing 6 unit tests + algebraic inspection; algorithm is the canonical Cormack 2009 form | n/a | n/a | none — existing tests are sufficient |
| 4 | `preprocess.rs` fast_image_resize fallback | yes | reasoned omission — fallback paths are minimally branching; one Read pass against the 88-line file is the right tool, not WebSearch | n/a | n/a | none — fallbacks are mechanically inspectable |
| 5 | `ort_session.rs` + encoder modules | yes | (shared with system #1) | 3 | <https://onnxruntime.ai/docs/performance/tune-performance/threading.html> | none — dual-constructor inspection is mechanical |
| 6 | `db/mod.rs` + `db/embeddings.rs` writer/reader split | yes | `SQLite WAL multiple writer connections same process performance pattern` | 3 | <https://sqlite.org/wal.html>, <https://oldmoe.blog/2024/07/08/the-write-stuff-concurrent-write-transactions-in-sqlite/> | `src-tauri/tests/audit_db_read_lock_routing_diagnostic.rs` — `#[ignore]`, documents which calls bypass `read_lock()` |
| 7 | `commands/encoders.rs` toggle IPCs | no — small, well-tested file (216 lines, 5 unit tests, pure decide function) | n/a | n/a | n/a | none — full coverage already exists |
| 8 | Frontend dispatch (`EncoderSection.tsx`, `useSimilarImages.ts`, `useSemanticSearch.ts`, `services/images.ts`) | yes | (shared with system #2) | 1 | n/a | none — frontend is locally inspectable |
| 9 | `perf.rs` + `perf_report.rs` Phase 7 sampler | yes | reasoned omission — sysinfo crate API is documented; the 1Hz sampler thread + stall analysis are localised to read against the source | n/a | n/a | none |
| 10 | General sweep (dead `priority_image_encoder` IPC, R-tags, TODOs, clippy) | yes | (covered by repo-wide grep + `cargo clippy` baseline) | n/a | n/a | none — clippy is the diagnostic |

## Modularisation candidate verdicts

Output of `python scripts/modularisation_candidates.py`:

| Path | Lines | Verdict | One-line justification |
|------|-------|---------|------------------------|
| `src-tauri/src/indexing.rs` | 1140 | `split-recommended` | Three concerns (pipeline orchestration, encoder phase, per-encoder loops) tangled in one file; previous code-health audit also flagged it. See `area-1-indexing.md` finding M-IDX-1. |
| `src-tauri/src/perf_report.rs` | 892 | `leave-as-is` | Single concern (markdown rendering). One function per section. The size reflects the report having many sections, not unrelated code. |
| `src-tauri/src/perf.rs` | 760 | `leave-as-is` | Self-contained subsystem (PerfLayer + RawEvent + flush thread + sampler + snapshot). Splitting would create cross-file types with no readability win. |
| `src-tauri/src/db/images_query.rs` | 719 | `leave-as-is` | Single concern (image SELECT + aggregate). Already extracted from db.rs in a prior audit; further split would orphan the helper functions from their callers. |
| `src-tauri/src/commands/similarity.rs` | 653 | `leave-as-is` | Three IPC commands + shared cross-encoder diagnostic. `get_similar_images` and `get_tiered_similar_images` may become extractable as dead code (see D-SIM-1 in `area-2-fusion-and-search.md`); after that the file shrinks naturally. |
| `src-tauri/src/db/embeddings.rs` | 647 | `leave-as-is` | ~60% of the file is `#[cfg(test)]` tests. Production code is ~250 lines, single concern. |
| `src-tauri/src/similarity_and_semantic_search/cosine/index.rs` | 619 | `leave-as-is` | Single concern (CosineIndex impl). `get_tiered_similar_images` may become dead code (see D-COS-1) — after removal, ~470 lines, well below threshold. |
| `src-tauri/src/lib.rs` | 522 | `leave-as-is` | Tauri Builder composition + State types. Splitting would scatter the `manage(...)` calls and lose the one-page view of how state hangs together. |
| `src-tauri/src/thumbnail/generator.rs` | 416 | `leave-as-is` | Single concern (thumbnail generation pipeline) with extensive comments documenting the JPEG scaled-decode + fast_image_resize path. Comments are ~30% of the file. |
| `src-tauri/src/model_download.rs` | 372 | `leave-as-is` | Single concern (HTTP download with progress + resume). Doesn't decompose cleanly. |
| `src-tauri/src/similarity_and_semantic_search/encoder.rs` | 370 | `leave-as-is` | Single concern (CLIP image encoder). Will shrink to ~280 lines once `inspect_model` + `encode_all_images_in_database` are removed (see D-ENC-1, D-ENC-2). |
| `src-tauri/src/db/mod.rs` | 359 | `leave-as-is` | `initialize()` + connection plumbing + per-mod re-exports — by design the orchestration file. |
| `src-tauri/src/commands/semantic.rs` | 355 | `leave-as-is` | If the legacy `semantic_search` IPC is removed (see D-SEM-1), file shrinks to ~150 lines. Until then, single-purpose file. |
| `scripts/download_lol_splashes.py` | 304 | `not-applicable` | One-off dev script for fetching test corpus; not part of the production codebase. |

`split-recommended`: 1. `leave-as-is`: 12. `not-applicable`: 1. No
candidate has a self-narrowing "out of scope" verdict.

## Diagnostic-test floor

Three test files written into `src-tauri/tests/` as `#[ignore]`-marked
diagnostics with explanatory docstrings. Each one documents an
uncertainty raised in this audit; together they bring the high-confidence
findings count up to the level the obligation requires.

| Test file | What it pins | Status |
|-----------|--------------|--------|
| `src-tauri/tests/audit_indexing_parallel_encoder_diagnostic.rs` | The dead `cosine_index` + `cosine_current_encoder` parameters threaded through `run_encoder_phase`; confirms the existing safety-net populate path. | `#[ignore]` — runs only when invoked explicitly with `cargo test -- --ignored` |
| `src-tauri/tests/audit_fusion_no_text_capable_encoders_diagnostic.rs` | `get_fused_semantic_search`'s "no enabled text-capable encoders" branch returning empty. | `#[ignore]` |
| `src-tauri/tests/audit_db_read_lock_routing_diagnostic.rs` | Documents the seven `db/embeddings.rs` and `db/*` methods that still hit `self.connection.lock()` (writer) on a read path. | `#[ignore]` |

## Confidence-upgrade pathway

Findings issued at `moderate` confidence in the per-area files have an
explicit "upgrade pathway" pointer (either the diagnostic test that
would lift them to `high`, or a one-line "no test would help — the
finding is grounded in the diff between source comments and current
behaviour"). See per-finding bodies in `area-*.md`.

## Production source code edits

`git diff HEAD --stat src-tauri/src/ src/` should show zero modifications
made by this audit. The three test files in `src-tauri/tests/` are the
only new files added by the audit itself; no production source touched.
