# Pass 2 Systems Audited — Code Health Audit

**Date:** 2026-04-26
**Status:** complete
**Pass 1 input:** `context/plans/code-health-audit/PASS-1-CHECKPOINT.md`

Pass 2 converted the enumerated surface into findings. The bar was intentionally conservative: a finding had to be behaviour-preserving by construction, backed by direct code evidence, or explicitly marked as possible behavioural impact requiring a decision. Documented feature work, such as SigLIP-2 text dispatch, was not re-labelled as a code-health cleanup.

## Systems Audited

| System | Files read / evidence | Research | Diagnostic status | Result |
|---|---|---|---|---|
| App shell / state wiring | `src-tauri/src/lib.rs`, `src-tauri/src/commands/roots.rs`, `src-tauri/src/commands/similarity.rs`, new ignored diagnostic test | Tauri state management docs | New diagnostic written | One high-priority correctness finding: empty cosine cache can remain marked current. |
| Indexing pipeline | `src-tauri/src/indexing.rs`, context indexing notes, Pass 1 script output | SQLite WAL docs | Existing pipeline tests are sufficient for current audit; no production change made | One modularisation finding: split private phases without changing public API. |
| Persistence/query layer | `src-tauri/src/db/images_query.rs`, Clippy output, DB tests baseline | rusqlite docs | No new diagnostic; Clippy and direct tuple-shape inspection are decisive | One cross-cutting/static-health finding: type-complexity cleanup belongs in the Clippy gate bundle. |
| Cosine retrieval | `src-tauri/src/similarity_and_semantic_search/cosine/index.rs`, `src-tauri/tests/cosine_topk_partial_sort_diagnostic.rs` | Rust slice partial-selection docs | Existing diagnostic confirms top-k optimisation; new cache diagnostic covers invalidation | No new algorithm finding; existing partial-selection design is sound. |
| Encoder stack | `src-tauri/src/similarity_and_semantic_search/encoder_text/encoder.rs`, `pooling.rs`, `commands/semantic.rs`, `context/systems/siglip2-encoder.md` | tokenizers docs, ONNX execution-provider docs | No new diagnostic; Clippy pinpoints a local allocation; SigLIP dispatch is feature work | One strict-Clippy cleanup item; no behaviour-changing SigLIP finding emitted. |
| Profiling / diagnostics | `src-tauri/src/perf.rs`, `src-tauri/src/perf_report.rs` | tracing-subscriber `Layer` docs | Existing unit tests cover the report/flush path | No split recommendation; Clippy cleanup included in cross-cutting finding. |
| Frontend route/state | `src/pages/[...slug].tsx`, query hooks, services imports | TanStack Query invalidation docs | No new diagnostic; extraction-only finding should run existing Vitest after implementation | One modularisation finding for route-level state extraction. |
| Model download / paths / watcher | `src-tauri/src/model_download.rs`, `src-tauri/src/paths.rs`, watcher context | notify-debouncer-mini docs | No new diagnostic; watcher rebuild is already documented and behavioural | No new system finding beyond strict-Clippy `paths.rs` cleanup. |
| Cross-cutting docs/dependencies | `context/architecture.md`, `context/notes.md`, `context/plans/pipeline-parallelism-and-stats-ui.md`, `package.json`, `package-lock.json` | local static evidence plus dependency tree | No new diagnostic; `rg` and `npm ls` are decisive | Findings for stale documentation and unused direct dev dependencies. |

## Diagnostic Test Written

`src-tauri/tests/cosine_cache_invalidation_diagnostic.rs`

This ignored test creates a fresh DB, populates two CLIP embeddings, loads the cosine cache, clears `cached_images` while leaving `current_encoder_id = "clip_vit_b_32"`, then calls `ensure_loaded_for` again. The current implementation returns early and leaves the cache empty. This models the root-command invalidation path exactly: `commands/roots.rs` clears the cache but does not clear the marker.

Validation:

- `cargo test --test cosine_cache_invalidation_diagnostic` passes because the demonstrator is ignored by default.
- `cargo test --test cosine_cache_invalidation_diagnostic -- --ignored` fails until the production bug is fixed, which makes it an acceptance test rather than a permanent failing baseline.

## Modularisation Verdicts

| Candidate | Verdict | Rationale |
|---|---|---|
| `src-tauri/src/indexing.rs` | split-recommended | One file owns model download, text prewarm, root scan, orphan marking, thumbnail work, encoder ordering, encoder loops, hot cache population, event emission, and tests. Extracting private phase modules would reduce edit blast radius while preserving `try_spawn_pipeline`. |
| `src-tauri/src/perf_report.rs` | leave-as-is | Long, but structured as a report renderer with section functions and local tests. Splitting now would mostly move private helpers around without reducing conceptual coupling. |
| `src-tauri/src/perf.rs` | leave-as-is | Cohesive collector/layer/session buffer module. It is large because it carries tests and comments explaining the telemetry contract. |
| `src-tauri/src/db/images_query.rs` | leave-as-is | Query layer is large but cohesive. The actionable issue is local type complexity in the aggregate row shape, not file decomposition. |
| `src-tauri/src/similarity_and_semantic_search/cosine/index.rs` | leave-as-is | Performance core is cohesive and already contains a reusable scratch buffer plus diagnostics. Splitting would risk hiding cache/scratch invariants. |
| `src-tauri/src/db/embeddings.rs` | leave-as-is | Cohesive embedding CRUD and tests. |
| `src-tauri/src/commands/similarity.rs` | leave-as-is | Command handler is verbose because diagnostics are rich; extraction would be reasonable later if diagnostics grow, but not required now. |
| `src-tauri/src/lib.rs` | leave-as-is | Central Tauri builder/startup file; command count drift is a documentation issue, not a split issue. |
| `src-tauri/src/model_download.rs` | leave-as-is | Constants + model download/checksum/progress behaviour belong together. |
| `src-tauri/src/similarity_and_semantic_search/encoder.rs` | leave-as-is | CLIP image encoder is cohesive. |
| `src-tauri/tests/similarity_integration_test.rs` | not-applicable | Test file. |
| `scripts/download_lol_splashes.py` | leave-as-is | Static orphan candidate, but explicitly documented as a reproducible test-corpus generator in `scripts/README.md`. |
| `src/pages/[...slug].tsx` | split-recommended | One route component owns selection, inspector notes, semantic search, similar search, folder add, settings/profiling shortcuts, and render sections. Extracting route hooks lowers regression risk. |
| `src/services/images.ts` | leave-as-is | Service facade is below the manual threshold and maps one IPC concern. |
| `src/components/PerfOverlay.tsx` | leave-as-is | Large but cohesive overlay component. |
| `src/components/SearchBar.tsx` | leave-as-is | Large but cohesive command/search UI. |
| `src/services/services.test.ts` | not-applicable | Test file. |
| `src/components/masonryPacking.test.ts` | not-applicable | Test file. |

## Reasoned Omissions

- SigLIP-2 text dispatch is already documented in `context/systems/siglip2-encoder.md` and `context/notes.md`. Wiring it changes runtime behaviour and state ownership, so it belongs to feature work rather than a behaviour-preserving health audit.
- Watcher rebuild after root add/remove/toggle is already documented in `context/systems/watcher.md`. Fixing it changes filesystem-event behaviour and should be handled as an explicit feature/bug-fix task, not hidden inside code-health cleanup.
- No production source was edited. The only code file added is an ignored diagnostic test.
