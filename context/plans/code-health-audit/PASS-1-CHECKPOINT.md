# Pass 1 Checkpoint — Code Health Audit (2026-04-25)

## Project understanding

Image Browser is a Tauri 2 + React 19 desktop application — a local-first Pinterest-style image grid with CLIP semantic search, multi-folder support, filesystem watching, persistent cosine cache, thumbnails, tags, and free-text annotations. All ML inference is on-device via ONNX Runtime. The repo has been through ~30 commits in the last day or two: phases 4-11 shipped together (folder picker, async indexing, multi-folder, watcher, settings drawer, image notes, AND/OR tags, comprehensive tests).

## Test-suite baseline

- **Backend (`cargo test` in `src-tauri/`):** 87 unit + 6 integration = 93 pass, 0 fail, 2 ignored (`test_real_image_similarity_search`, `test_similarity_distribution`). The two ignored tests are intentionally `#[ignore]`'d because they require real CLIP-encoded image fixtures.
- **Frontend (`npm test` → vitest):** 4 files, 53 tests pass, 0 fail.
- **Total: 146 tests, all passing.**
- Baseline is healthy. No pre-existing test failures to record as Known Issues.

## Documentation rot detected at the project level

The architecture and systems docs in `context/` describe the codebase as it was 16 commits ago. The reality has moved substantially:

| Doc claim | Reality |
|-----------|---------|
| 14 Rust source files | 14 Rust source files in `src-tauri/src/`, but the *set* differs — `indexing.rs`, `model_download.rs`, `paths.rs`, `root_struct.rs`, `settings.rs`, `watcher.rs` exist; the architecture lists none of them. |
| 8 Tauri commands | 17 Tauri commands (`get_images`, `get_tags`, `create_tag`, `delete_tag` is wired now, `add_tag_to_image`, `remove_tag_from_image`, `get_similar_images`, `get_tiered_similar_images`, `semantic_search`, `get_scan_root`, `set_scan_root`, `list_roots`, `add_root`, `remove_root`, `set_root_enabled`, `get_image_notes`, `set_image_notes`). |
| `db.rs` ~1150 lines | 1597 lines |
| `lib.rs` ~600 lines | 918 lines |
| `cosine_similarity.rs` 3 retrieval modes | Still 3 modes, but it has grown to 822 lines because of the persistent on-disk cache (save/load, version handling, mtime check). |
| Hardcoded test_images path | Replaced by user-configured `roots` table + persisted `settings.json` + native folder picker (`tauri-plugin-dialog`). |
| `images.db` at repo root | Lives in `<repo>/Library/` per commit `3c2900f`. |
| `delete_tag` is implemented but not registered | Now registered; UI affordance shipped. |
| Tag filter is OR-only | AND/OR toggle shipped per commit `56990b7`. |
| `println!` for backend logging | Replaced by `tracing::info!` / `tracing::debug!` / `tracing::error!` per commit `7918e39`. |
| `populate_from_db(db_path: &str)` opens its own connection | Refactored to take `&ImageDatabase` per the lib.rs signatures — the path-and-state-coupling smell is partially resolved. |

The `context/` documentation update is itself a **Documentation Rot finding** but is also a pre-condition for any future audit being grounded. Logged as such in the cross-cutting plan file.

## Modularisation candidate list

Threshold: Rust ≥350 lines OR top decile (top 3 of 21 Rust files). Top decile alone would catch the first 3; the threshold catches three more.

| File | Lines | Qualifies because |
|------|-------|-------------------|
| `src-tauri/src/db.rs` | 1597 | top-decile + ≥350 |
| `src-tauri/src/lib.rs` | 918 | top-decile + ≥350 |
| `src-tauri/src/similarity_and_semantic_search/cosine_similarity.rs` | 822 | top-decile + ≥350 |
| `src-tauri/src/similarity_and_semantic_search/encoder_text.rs` | 647 | ≥350 |
| `src-tauri/src/indexing.rs` | 589 | ≥350 |
| `src-tauri/tests/similarity_integration_test.rs` | 350 | ≥350 |
| `scripts/download_lol_splashes.py` | 304 | ≥300 (Python threshold) |
| `src/components/SettingsDrawer.tsx` | 466 | ≥300 (TS threshold per SKILL.md, and this is the largest TS file) |
| `src/pages/[...slug].tsx` | 392 | ≥300 |
| `src/services/images.ts` | 275 | very close to threshold — included for inspection |
| `src/components/SearchBar.tsx` | 252 | very close to threshold — included for inspection |

The `modularisation_candidates.py` script does not consider TypeScript; the last four rows were added manually using `wc -l`. Script invocation is recorded in the Obligation Evidence Map.

Per-file modularisation verdicts will be issued in `modularisation.md` and recorded in PASS-2-SYSTEMS-AUDITED.md.

## Hotspot-intersection signal (composite ≥ 0.80 = near-certain Pass-2 target)

| Rank | File | Composite |
|-----:|------|----------:|
| 1 | `src-tauri/src/db.rs` | 0.98 |
| 2 | `src-tauri/src/lib.rs` | 0.97 |
| 3 | `src-tauri/src/similarity_and_semantic_search/cosine_similarity.rs` | 0.84 |
| 4 | `src-tauri/src/indexing.rs` | 0.76 |
| 5 | `src-tauri/src/paths.rs` | 0.75 |
| 6 | `src-tauri/src/similarity_and_semantic_search/encoder_text.rs` | 0.73 |
| 7 | `src-tauri/src/similarity_and_semantic_search/encoder.rs` | 0.62 |
| 8 | `src-tauri/src/model_download.rs` | 0.59 |

## Orphan-detection signal

Only one orphan (`scripts/download_lol_splashes.py`). Script confirms it has fan-in 0; this is intentional — it's a dev-tools script. No Dead-Code finding from this signal alone.

## Pass-2 prioritisation (with rationale)

| Order | System / file | Why this rank |
|------:|---------------|---------------|
| 1 | `db.rs` (database) | Highest composite (0.98). User explicitly flagged the size. Also has the unsafe BLOB cast, AND/OR semantics, root cascading — many findings expected. |
| 2 | `lib.rs` (tauri-commands) | Composite 0.97. Triplicated `normalize_path` is still in there (verified visually). 17 commands. User flagged the size. |
| 3 | `cosine_similarity.rs` (cosine + persistent cache) | Composite 0.84. New persistent cache adds I/O surface. Three retrieval modes share path → id mapping. User flagged the size. |
| 4 | `indexing.rs` (async pipeline + orphan detection) | New code, composite 0.76. Owns the multi-step pipeline + state machine + filesystem-watcher integration. |
| 5 | `lib.rs` again, but for the path-id mapping (cross-cutting with cosine + semantic_search) | The triplicated path normalisation closure is the most-flagged refactor in the project notes; address it once, not three times. |
| 6 | `encoder_text.rs` (CLIP text encoder + tokenizer) | 647 lines, recent CoreML fallback churn, mean-pool path; not in the user's flagged list but substantive. |
| 7 | `encoder.rs` (CLIP image encoder) | Image preprocessing quality concerns documented in `notes/clip-preprocessing-decisions.md`; check whether they're still applicable. |
| 8 | `indexing.rs` orphan-detection logic | Filesystem walks + DB joins. Worth a focused performance read. |
| 9 | Frontend `[...slug].tsx` and routing | UX bugs documented in `systems/search-routing.md` (selectedItem lookup, arrow nav). |
| 10 | `SettingsDrawer.tsx` (466 lines) | Modularisation candidate. |
| 11 | Cross-cutting: documentation rot, dead code per the existing `dead-code-inventory.md` (which itself is partially stale post-cleanup). |

## Known issues already surfaced from context files

Per `notes/dead-code-inventory.md`, `notes/path-and-state-coupling.md`, `notes/mutex-poisoning.md`, `notes/clip-preprocessing-decisions.md`, and the 11 `systems/*.md` files, these candidate findings will be re-validated during Pass 2:

- Dead code: `FullscreenImage.tsx`, `MasonryItemSelected.tsx`, `MasonrySelectedFrame.tsx`, `useMeasure.tsx`, `useSimilarImages` hook, `ImageData::with_thumbnail`, `zustand`, `atropos` CSS.
- Path-coupling: triplicated `normalize_path` closure in 3 commands.
- Mutex poisoning posture (informational, not actionable as a free finding).
- CLIP preprocessing — `FilterType::Nearest` + ImageNet stats.
- N+1 query in `populate_from_db` (one `get_image_embedding` per row).
- Tag colour inconsistency (`#3489eb` vs `#3B82F6`).
- `add_tag_to_image` uses `INSERT` not `INSERT OR IGNORE`.
- `searchText` in the `useImages` query key is wasted (backend ignores it).
- Selection-lookup-via-`images.data` UX gap when result comes from semantic search.

Each of these will get re-checked against current code reality; some may already be fixed (e.g. delete_tag is now wired).

## Stack-fit notes

- **Project language:** Rust + TypeScript + React. The `modularisation_candidates.py` script catches Python and Rust; for TS files we fall back to `wc -l` + `Glob`. Recorded as a reasoned omission in the Obligation Evidence Map for the TS files.
- **Test command:** `cargo test` for Rust, `npm test` (`vitest run`) for frontend. Both were run live.
- **Build system:** `cargo` workspace inside `src-tauri/`, Vite for frontend. `cargo build` works; `npm run build` builds the bundle.
