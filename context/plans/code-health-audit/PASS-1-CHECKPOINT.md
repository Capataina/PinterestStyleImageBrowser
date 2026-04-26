# Pass 1 checkpoint — 2026-04-26 audit

Snapshot at the close of orientation, before per-system deep dive begins.

## Project model

- Stack: Tauri 2 + React 19 + Rust + ONNX (`ort = 2.0.0-rc.10`) +
  SQLite (rusqlite, WAL).
- 12,938 lines of Rust across 57 files in `src-tauri/src/`.
- 6,403 lines of TypeScript across 33 files in `src/`.
- 26 Tauri command IPC surface, three image encoders (CLIP + DINOv2 +
  SigLIP-2), Reciprocal Rank Fusion across encoders for both
  image-image and text-image search.
- Recent perf work landed in 12 commits (`f5706ed` → `1ca42d2`).

## Test-suite baseline

| Suite | Command | Result |
|-------|---------|--------|
| Cargo lib tests | `cargo test --manifest-path src-tauri/Cargo.toml --lib` | **125 passed; 0 failed; 0 ignored**, 1.16 s |
| Vitest | `npm test --silent -- --run` | **62 passed**, 944 ms |
| Clippy | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | **clean, 0 warnings** |
| TypeScript | `npx tsc --noEmit` | **clean, 0 errors** |

No pre-existing test failures. No Known Issues findings derived from a
broken baseline.

## Systems prioritisation for Pass 2

Ranked by likelihood of yielding free findings (highest first):

1. **`indexing.rs`** — heavy churn, parallel encoder phase, `super::`
   path inconsistency, dead arguments.
2. **`commands/{semantic_fused,semantic,similarity}.rs`** — fusion +
   legacy paths coexist; need to verify which legacy paths are reachable.
3. **Frontend `services/images.ts` + `useSimilarImages.ts` +
   `useSemanticSearch.ts`** — confirm legacy IPCs are dead from the UI.
4. **`db/mod.rs` + `db/embeddings.rs`** — R2 read-only secondary
   convention adherence.
5. **Encoder modules** (`encoder.rs`, `encoder_dinov2.rs`,
   `encoder_siglip2.rs`, `encoder_text/encoder.rs`) — dual `new` /
   `new_with_intra` constructors; verify which `new()` are dead.
6. **`cosine/rrf.rs`** — RRF math correctness, edge cases.
7. **`commands/encoders.rs`** — already well-tested; quick read.
8. **`preprocess.rs`** — small file, fallback paths.
9. **`perf.rs` + `perf_report.rs`** — Phase 7 sampler thread.
10. **`settings.rs` + `lib.rs`** — `priority_image_encoder` deprecation
    contradiction.

## Modularisation candidate list

Output of `python scripts/modularisation_candidates.py`:

| Path | Lines | Qualifies because |
|------|-------|-------------------|
| `src-tauri/src/indexing.rs` | 1140 | ≥350 lines, top-decile |
| `src-tauri/src/perf_report.rs` | 892 | ≥350 lines, top-decile |
| `src-tauri/src/perf.rs` | 760 | ≥350 lines, top-decile |
| `src-tauri/src/db/images_query.rs` | 719 | ≥350 lines, top-decile |
| `src-tauri/src/commands/similarity.rs` | 653 | ≥350 lines, top-decile |
| `src-tauri/src/db/embeddings.rs` | 647 | ≥350 lines, top-decile |
| `src-tauri/src/similarity_and_semantic_search/cosine/index.rs` | 619 | ≥350 lines |
| `src-tauri/src/lib.rs` | 522 | ≥350 lines |
| `src-tauri/src/thumbnail/generator.rs` | 416 | ≥350 lines |
| `src-tauri/src/model_download.rs` | 372 | ≥350 lines |
| `src-tauri/src/similarity_and_semantic_search/encoder.rs` | 370 | ≥350 lines |
| `src-tauri/src/db/mod.rs` | 359 | ≥350 lines |
| `src-tauri/src/commands/semantic.rs` | 355 | ≥350 lines |
| `scripts/download_lol_splashes.py` | 304 | ≥300 lines, top-decile |

Per-file verdict (per the modularisation evaluation floor) lives in
`obligation-evidence-map.md` § "Modularisation candidate verdicts".

## Known issues already surfaced from context

- `notes/notes.md` flags **indexing.rs phase-module split** as a
  code-health audit medium item — already in the queue.
- `notes/dead-code-inventory.md` flags `dinov2_small` legacy id and
  some residual dead code from prior phases.
- `notes/encoder-additions-considered.md` is forward-looking research
  — no findings owed.

## Pass-2 focus

Production source remains read-only throughout Pass 2. Findings are
written to `area-*.md`; diagnostic tests go to `src-tauri/tests/`.
