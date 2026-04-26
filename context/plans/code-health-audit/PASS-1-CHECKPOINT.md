# Pass 1 Checkpoint — Code Health Audit

**Date:** 2026-04-26
**Scope:** full repository (`src-tauri/` Rust backend, `src/` React frontend, scripts, context plans/docs)
**Status:** complete — Pass 2 may begin

## Context Read

- `context/architecture.md` — project map, subsystem ownership, critical paths, current multi-encoder architecture.
- `context/notes.md` — active work areas, conventions, known gaps.
- `context/systems/*.md` — all system docs enumerated and headings/known-risk sections scanned for Pass-2 prioritisation.
- `context/plans/perf-diagnostics.md` and `context/plans/pipeline-parallelism-and-stats-ui.md` — active plan state checked for stale or in-progress work.
- `README.md` was already read during session orientation; it remains directionally older than the current context for multi-encoder reality.

## Test / Build Baseline

| Command | Result | Notes |
|---|---|---|
| `bash /Users/atacanercetinkaya/.codex/skills/code-health-audit/scripts/test_baseline.sh /Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser` | pass | Script detected Node; Vitest: 4 files passed, 53 tests passed. |
| `cargo test` from `src-tauri/` | pass with warning | 107 lib tests + 6 cosine diagnostic + 6 indexing pipeline tests passed; 2 real-image integration tests ignored. Warning: unused import `super::*` in `src/db/roots.rs:145`. |
| `npm run build` | pass | `tsc && vite build` succeeded; generated Vite chunks. |
| `cargo clippy --all-targets --all-features -- -D warnings` | fail | 33 clippy errors. Mostly low-risk cleanup, but also `type_complexity` in `db/images_query.rs`, unnecessary allocation in `encoder_text/encoder.rs`, and documentation/maintainability drift in encoder modules. |

Pre-existing failures:

- No unit/integration test failures.
- Strict Clippy is not clean; this becomes a cross-cutting code health finding rather than a test baseline failure.

## Script Evidence

### Modularisation Candidates Script

Command:

```bash
python3 /Users/atacanercetinkaya/.codex/skills/code-health-audit/scripts/modularisation_candidates.py /Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser
```

Script fallback note: the skill script invocation in `SKILL.md` says `python`, but this machine exposes Python as `python3`; `python` returned `command not found`.

Output candidates:

| Path | Language | Lines | Qualifies because |
|---|---|---:|---|
| `src-tauri/src/indexing.rs` | Rust | 1016 | `>=350`, top-decile |
| `src-tauri/src/perf_report.rs` | Rust | 716 | `>=350`, top-decile |
| `src-tauri/src/perf.rs` | Rust | 697 | `>=350`, top-decile |
| `src-tauri/src/db/images_query.rs` | Rust | 646 | `>=350`, top-decile |
| `src-tauri/src/similarity_and_semantic_search/cosine/index.rs` | Rust | 611 | `>=350`, top-decile |
| `src-tauri/src/db/embeddings.rs` | Rust | 469 | `>=350`, top-decile |
| `src-tauri/src/commands/similarity.rs` | Rust | 440 | `>=350` |
| `src-tauri/src/lib.rs` | Rust | 377 | `>=350` |
| `src-tauri/src/model_download.rs` | Rust | 372 | `>=350` |
| `src-tauri/src/similarity_and_semantic_search/encoder.rs` | Rust | 357 | `>=350` |
| `src-tauri/tests/similarity_integration_test.rs` | Rust | 350 | `>=350` |
| `scripts/download_lol_splashes.py` | Python | 304 | `>=300`, top-decile |

### TypeScript Modularisation Fallback

Command:

```bash
find src -type f \( -name '*.ts' -o -name '*.tsx' \) -exec wc -l {} + | sort -nr
```

The Python/Rust scripts do not enumerate TypeScript. Applying the skill threshold manually (`>=300` lines OR top decile of 54 TS/TSX files) yields:

| Path | Language | Lines | Qualifies because |
|---|---|---:|---|
| `src/pages/[...slug].tsx` | TypeScript/React | 516 | `>=300`, top-decile |
| `src/services/images.ts` | TypeScript | 287 | top-decile |
| `src/components/PerfOverlay.tsx` | TypeScript/React | 269 | top-decile |
| `src/components/SearchBar.tsx` | TypeScript/React | 252 | top-decile |
| `src/services/services.test.ts` | TypeScript test | 248 | top-decile |
| `src/components/masonryPacking.test.ts` | TypeScript test | 225 | top-decile |

### Import Graph

Command:

```bash
python3 /Users/atacanercetinkaya/.codex/skills/code-health-audit/scripts/import_graph.py /Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser --top 30
```

Highest fan-in:

- `src-tauri/src/db/mod.rs` — fan-in 23, fan-out 9.
- `src-tauri/src/lib.rs` — fan-in 15, fan-out 18.
- `src-tauri/src/paths.rs` — fan-in 9, fan-out 1.
- `src-tauri/src/commands/mod.rs` — fan-in 8, fan-out 12.
- `src-tauri/src/perf.rs` — fan-in 5, fan-out 1.

Highest fan-out:

- `src-tauri/src/lib.rs` — fan-out 18.
- `src-tauri/src/commands/mod.rs` — fan-out 12.
- `src-tauri/src/db/mod.rs` — fan-out 9.
- `src-tauri/src/indexing.rs` — fan-out 9.
- `src-tauri/src/commands/roots.rs` — fan-out 7.

### Hotspot Intersection

Command:

```bash
python3 /Users/atacanercetinkaya/.codex/skills/code-health-audit/scripts/hotspot_intersect.py /Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser --top 25
```

Composite `>=0.80` Pass-2 targets:

| Rank | Path | Lines | Fan-in | Churn | Composite |
|---:|---|---:|---:|---:|---:|
| 1 | `src-tauri/src/lib.rs` | 377 | 15 | 23 | 0.95 |
| 2 | `src-tauri/src/indexing.rs` | 1016 | 3 | 13 | 0.92 |
| 3 | `src-tauri/src/perf.rs` | 697 | 5 | 4 | 0.88 |
| 4 | `src-tauri/src/paths.rs` | 252 | 9 | 10 | 0.85 |
| 5 | `src-tauri/src/db/images_query.rs` | 646 | 2 | 4 | 0.82 |
| 6 | `src-tauri/src/model_download.rs` | 372 | 2 | 8 | 0.80 |

### Orphan Candidate Sweep

Command:

```bash
python3 /Users/atacanercetinkaya/.codex/skills/code-health-audit/scripts/orphans.py /Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser
```

Candidate:

- `scripts/download_lol_splashes.py` — 304 lines, Python, fan-in 0. This is a developer utility documented under `scripts/README.md`, so Pass 2 must classify it explicitly rather than deleting by static fan-in alone.

## Systems Identified

| System | Primary files | Pass-2 priority | Rationale |
|---|---|---:|---|
| App shell / Tauri state wiring | `src-tauri/src/lib.rs`, `main.rs`, `commands/*` | 1 | Highest hotspot score; central state ownership; command registration and startup side effects. |
| Indexing pipeline | `src-tauri/src/indexing.rs`, `filesystem.rs`, `thumbnail/generator.rs`, encoder modules | 2 | Largest file, high churn, concurrency + DB + ONNX + progress events. |
| Persistence and query layer | `src-tauri/src/db/*` | 3 | Highest fan-in; query performance and schema migration safety drive most backend paths. |
| Cosine / similarity retrieval | `src-tauri/src/similarity_and_semantic_search/cosine/*`, `commands/similarity.rs` | 4 | Hot path for search quality/performance; existing diagnostics and partial-sort work need audit follow-up. |
| Encoder stack and semantic dispatch | `encoder.rs`, `encoder_text/*`, `encoder_dinov2.rs`, `encoder_siglip2.rs`, `commands/semantic.rs` | 5 | Current active gap around text encoder dispatch; high cost ONNX paths. |
| Profiling / diagnostics | `perf.rs`, `perf_report.rs`, `PerfOverlay.tsx`, `services/perf.ts` | 6 | Large new subsystem; clippy and modularisation candidates; opt-in but important for future debugging. |
| Frontend routing/state | `src/pages/[...slug].tsx`, `queries/*`, `hooks/*`, `services/*`, `components/settings/*` | 7 | Largest frontend file; owns search-routing and user-visible state correctness. |
| Model download and paths/state | `model_download.rs`, `paths.rs`, `settings.rs` | 8 | First-launch reliability, path correctness, large model URLs, app-data semantics. |
| Cross-cutting hygiene | manifests, scripts, context plans/docs | 9 | Clippy failures, dependency usage, orphan candidates, stale plan/doc state. |

## Known Issues Already Surfaced From Context

- SigLIP-2 text encoder is implemented but semantic-search runtime dispatch still hardcodes CLIP. This is documented in `context/systems/siglip2-encoder.md` and `context/notes.md`.
- Watcher does not rebuild when roots are added/removed/toggled after app launch. This is documented in `context/systems/watcher.md` and `context/systems/indexing.md`.
- Path normalisation at insert time remains open. This is documented in `context/notes/path-and-state-coupling.md`.
- Smart query-based encoder routing is deliberately deferred. This is documented in `context/notes/preprocessing-spatial-coverage.md`.
- Several context plan/docs are stale relative to the committed code: `context/plans/pipeline-parallelism-and-stats-ui.md` still lists stats UI and parallel pipeline work as unchecked/missing, despite code implementing both broad pieces.

## Modularisation Candidate Verdicts Pending Pass 2

Every candidate below must receive `split-recommended`, `leave-as-is`, or `not-applicable` in Pass 2.

| Candidate | Verdict | Placeholder rationale |
|---|---|---|
| `src-tauri/src/indexing.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/perf_report.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/perf.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/db/images_query.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/similarity_and_semantic_search/cosine/index.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/db/embeddings.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/commands/similarity.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/lib.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/model_download.rs` | PENDING | Pass-2 read required. |
| `src-tauri/src/similarity_and_semantic_search/encoder.rs` | PENDING | Pass-2 read required. |
| `src-tauri/tests/similarity_integration_test.rs` | PENDING | Pass-2 read required. |
| `scripts/download_lol_splashes.py` | PENDING | Pass-2 read required. |
| `src/pages/[...slug].tsx` | PENDING | Pass-2 read required. |
| `src/services/images.ts` | PENDING | Pass-2 read required. |
| `src/components/PerfOverlay.tsx` | PENDING | Pass-2 read required. |
| `src/components/SearchBar.tsx` | PENDING | Pass-2 read required. |
| `src/services/services.test.ts` | PENDING | Pass-2 read required. |
| `src/components/masonryPacking.test.ts` | PENDING | Pass-2 read required. |

