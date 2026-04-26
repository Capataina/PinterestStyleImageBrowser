# Architecture

## Scope / Purpose

Image Browser is a local-first Tauri 2 desktop application for browsing, tagging, semantically searching, and annotating large personal image libraries. The Rust backend handles filesystem scanning, SQLite persistence (WAL), thumbnail generation, ONNX-Runtime inference across **three encoder families** (CLIP ViT-B/32 OpenAI English, DINOv2-Base, SigLIP-2 Base 256), multi-folder lifecycle, a filesystem watcher with orphan detection, an opt-in profiling + domain-diagnostic layer, and first-launch model downloads from HuggingFace. The React 19 frontend renders a Pinterest-style masonry grid, a modal inspector with annotations, a settings drawer (with per-direction encoder picker), a live indexing-status pill, and an optional perf-overlay. Everything runs offline on consumer hardware; backend runs CPU on macOS for ONNX (CoreML produces runtime errors for these models) and tries CUDA on non-macOS, with CPU fallback.

This document is the structural map. Subsystem-level reality lives in `systems/`. Project-level rationale and conventions live in `notes/`. Active work plans live in `plans/`.

## Repository Overview

| Dimension | Value | Source |
|-----------|-------|--------|
| Cargo package | `image-browser` v0.1.0, edition 2021 | `src-tauri/Cargo.toml` |
| Tauri identifier | `com.ataca.image-browser` | `src-tauri/tauri.conf.json` |
| Frontend bundler | Vite 7 + `vite-plugin-pages` (file-based routing) | `vite.config.ts`, `package.json` |
| Backend source | 28 Rust files in `src-tauri/src/` (added `ort_session.rs`, `cosine/rrf.rs`) | filesystem |
| Frontend source | 33 TypeScript files in `src/` (incl. settings/ subcomponents) | filesystem |
| Persistence | Single SQLite file `Library/images.db`, **WAL journal mode**, **two connections per real DB** (writer + read-only secondary), 5 tables. | `src-tauri/src/db/mod.rs` |
| SQLite PRAGMAs | `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`, `wal_autocheckpoint=0` (manual via `checkpoint_passive` between encoder batches), `journal_size_limit=64 MiB`, `foreign_keys=ON`. | `db/mod.rs::initialize` |
| ML runtime | `ort = 2.0.0-rc.10` with shared M2-tuned `Session` builder (`Level3 + intra_threads(4) + inter_threads(1)`). CPU on macOS for all three encoder families (CoreML produces runtime inference errors for these graphs); CUDA on non-macOS with CPU fallback. | `Cargo.toml`, `similarity_and_semantic_search/ort_session.rs` |
| Image encoders | **CLIP ViT-B/32** (OpenAI English, separate `vision_model.onnx`, 512-d), **DINOv2-Base** (Meta self-supervised, 768-d, image-only), **SigLIP-2 Base 256** (Google sigmoid loss, 768-d shared text+image space). Picker UI in Settings selects per direction. | `similarity_and_semantic_search/encoder*.rs` |
| Text encoders | **CLIP ViT-B/32** (512-d, BPE 77 tokens, real-input pre-warm via `encode("warmup")`). **SigLIP-2** (Gemma SentencePiece 256k vocab, 64 tokens, NO attention_mask). **Both wired through `semantic_search`'s `text_encoder_id` parameter** — the picker actually dispatches now. | `encoder_text/encoder.rs`, `encoder_siglip2.rs`, `commands/semantic.rs` |
| Tokenizer | HuggingFace `tokenizers = "0.22.2"` crate handles BPE (CLIP) and SentencePiece (SigLIP-2) uniformly via `tokenizer.json`. | `Cargo.toml` |
| Multi-encoder fusion | **Reciprocal Rank Fusion** (Cormack 2009, k=60) across CLIP + SigLIP-2 + DINOv2 for image-image similarity. Per-encoder cosine caches resident in `FusionIndexState`. Replaces the previous tiered random-sampling diversity strategy. | `similarity_and_semantic_search/cosine/rrf.rs`, `commands/similarity.rs::get_fused_similar_images`, `lib.rs::FusionIndexState` |
| Encoder write path | Encoder pipeline writes embeddings via `upsert_embeddings_batch` (one `BEGIN IMMEDIATE` per chunk of ~32 rows), with `checkpoint_passive` between batches. Replaces the previous per-row `upsert_embedding` autocommit pattern that triggered multi-second checkpoint stalls. | `db/embeddings.rs::upsert_embeddings_batch`, `indexing.rs::run_clip_encoder` + `run_trait_encoder` |
| Thumbnail pipeline | JPEG sources go through `jpeg-decoder::Decoder::scale()` for native scaled IDCT (1/8, 1/4, 1/2 factor), then `fast_image_resize 6.x` (NEON-optimised Lanczos3) for the final downsample. Falls back to `image-rs` for non-JPEG and any decode error. | `thumbnail/generator.rs` |
| Tauri commands | **26**, grouped by concern under `commands/` (images, tags, notes, roots, similarity, semantic, profiling, encoders) — added `get_fused_similar_images` for RRF dispatch. | `lib.rs::run` invoke_handler |
| Typed errors | `ApiError` discriminated union; mirrored on the frontend in `services/apiError.ts`. | `commands/error.rs` |
| Profiling + diagnostics | Opt-in via `--profile` CLI flag. PerfLayer (span timing) + `record_diagnostic` (12 named diagnostics + the new `system_sample` from the 1Hz RSS/CPU sampler). On-exit report includes Stall Analysis + Resource Trends sections. Off by default — zero overhead. | `main.rs`, `perf.rs`, `perf_report.rs`, `cosine/diagnostics.rs` |
| User state | `<repo>/Library/` in dev (`debug_assertions`); platform app-data dir in release. | `src-tauri/src/paths.rs` |
| Models on disk | `Library/models/{clip_vision.onnx, clip_text.onnx, clip_tokenizer.json, dinov2_base_image.onnx, siglip2_vision.onnx, siglip2_text.onnx, siglip2_tokenizer.json}` (~2.5 GB total — all FP32, no quantization). Per-encoder fail-soft download. | `src-tauri/src/model_download.rs` |
| Filesystem watcher | `notify-debouncer-mini`, 5s debounce, recursive on every enabled root | `src-tauri/src/watcher.rs` |
| Embedding-pipeline migration | `meta(key, value)` table tracks `embedding_pipeline_version`. Bumping the const wipes legacy embeddings + per-encoder rows on first launch under the new code. **Currently version 3** (bumped 2026-04-26 with R6+R7+R8 — the resize backend swap + scaled JPEG decode change preprocessed RGB buffers, and R8 stops writing the legacy `images.embedding` column). | `db/schema_migrations.rs::migrate_embedding_pipeline_version` |

## Repository Structure

```text
PinterestStyleImageBrowser/
├── README.md                   # Project intent & milestone roadmap
├── package.json                # React 19, TanStack Query 5, framer-motion, lucide-react, shadcn primitives, vitest
├── tsconfig.json               # @/ alias → /src
├── vite.config.ts              # Tailwind v4, @vitejs/plugin-react, vite-plugin-pages
├── vitest.config.ts            # JSDOM env, src/test/setup.ts
├── components.json             # shadcn-ui registry config
├── public/                     # Static assets served by Vite
├── Library/                    # User state (gitignored): images.db, settings.json, cosine_cache.bin,
│                               # models/, thumbnails/root_<id>/, exports/perf-<unix_ts>/
├── scripts/                    # Dev tooling (LoL splash downloader for test corpus)
├── src/                        # Frontend (React)
│   ├── App.tsx                 # BrowserRouter + QueryClientProvider + Pages routes
│   ├── main.tsx                # ReactDOM root, theme pre-flush before mount
│   ├── pages/[...slug].tsx     # Single catch-all route — URL slug = selected image id; owns search-routing
│   ├── components/
│   │   ├── Masonry.tsx         # Shortest-column packing; promotes hero across up to 3 cols; sortMode-aware
│   │   ├── MasonryItem.tsx     # 3D tilt via framer-motion; honours animationLevel pref
│   │   ├── MasonryAnchor.tsx   # Absolute-positioned wrapper used by Masonry
│   │   ├── PinterestModal.tsx  # Fullscreen inspector with prev/next, tag editing, notes textarea
│   │   ├── SearchBar.tsx       # # autocomplete + tag pills + create-tag-on-no-match + delete-tag affordance
│   │   ├── TagDropdown.tsx     # Popover combobox (cmdk)
│   │   ├── IndexingStatusPill.tsx  # Floating top-right pill driven by indexing-progress events
│   │   ├── PerfOverlay.tsx     # Profiling-mode-only diagnostics panel (cmd+shift+P)
│   │   ├── settings/           # Settings drawer (split per audit finding)
│   │   │   ├── index.tsx       # Slide-in shell, esc/backdrop dismiss
│   │   │   ├── controls.tsx    # Shared section header + slider/toggle primitives
│   │   │   ├── ThemeSection.tsx
│   │   │   ├── DisplaySection.tsx
│   │   │   ├── SearchSection.tsx
│   │   │   ├── SortSection.tsx
│   │   │   ├── FoldersSection.tsx
│   │   │   └── ResetSection.tsx
│   │   └── ui/                 # shadcn primitives — derivative
│   ├── queries/                # TanStack Query hooks (one file per resource family)
│   │   ├── queryClient.ts      # staleTime: Infinity, no auto-refetch
│   │   ├── useImages.ts        # useImages + useAssignTagToImage + useRemoveTagFromImage (optimistic)
│   │   ├── useTags.ts          # useTags + useCreateTag + useDeleteTag (optimistic)
│   │   ├── useRoots.ts         # useRoots + add/remove/setEnabled mutations
│   │   ├── useSimilarImages.ts # useTieredSimilarImages
│   │   └── useSemanticSearch.ts# 5-min staleTime, 10-min gcTime, debounced from caller
│   ├── services/               # invoke() wrappers — translate Tauri JSON to UI types via ApiError
│   │   ├── apiError.ts         # ApiError discriminated union + formatApiError() + isMissingModelError()
│   │   ├── images.ts           # fetchImages, fetchTieredSimilarImages, semanticSearch, pickScanFolder, setScanRoot, getThumbnailPath
│   │   ├── tags.ts             # fetchTags, createTag, deleteTag
│   │   ├── notes.ts            # getImageNotes, setImageNotes
│   │   ├── roots.ts            # listRoots, addRoot, removeRoot, setRootEnabled
│   │   └── perf.ts             # isProfilingEnabled, getPerfSnapshot, recordAction, exportPerfSnapshot, perfInvoke wrapper
│   ├── hooks/
│   │   ├── useDebouncedValue.ts  # 300ms debounce
│   │   ├── useUserPreferences.ts # localStorage-backed prefs (theme, columns, sort, animation, search counts, tagFilterMode)
│   │   └── useIndexingProgress.ts# Subscribes to the `indexing-progress` Tauri event
│   ├── lib/utils.ts            # cn() helper for shadcn
│   ├── utils.ts                # getImageSize() via DOM Image, waitForAllInnerImages()
│   └── types.d.ts              # ImageData, ImageItem, Tag, Root, SimilarImageItem, SemanticSearchResult
└── src-tauri/                  # Rust backend + Tauri shell
    ├── Cargo.toml              # ort, rusqlite, tauri (+plugin-dialog, +plugin-opener), image, ndarray, rand,
    │                           # rayon, notify, notify-debouncer-mini, ureq, tracing/tracing-subscriber,
    │                           # bytemuck, dirs, serde, fast_image_resize 6 (R6 thumbnail resize),
    │                           # jpeg-decoder 0.3 (R7 scaled IDCT), sysinfo 0.32 (Phase 7 RSS/CPU sampler)
    ├── tauri.conf.json         # csp: null, assetProtocol scope ["**"]
    └── src/
        ├── main.rs             # `--profile` parsing, tracing subscriber + opt-in PerfLayer, DB open + initialize, hands to lib::run
        ├── lib.rs              # State types (CosineIndexState, TextEncoderState{clip+siglip2}, FusionIndexState), run(): tauri::Builder.manage(...)
        │                       # .setup(legacy migrate + spawn pipeline + start watcher).invoke_handler![26 commands].run() with on-Exit perf report hook
        ├── commands/           # Tauri command handlers, grouped by concern (audit modularisation)
        │   ├── mod.rs          # Re-exports + ImageSearchResult unified struct + resolve_image_id_for_cosine_path helper
        │   ├── error.rs        # ApiError enum with `#[serde(tag="kind", content="details")]`; From-impls for rusqlite/io/poison
        │   ├── images.rs       # get_images
        │   ├── tags.rs         # get_tags, create_tag, delete_tag, add_tag_to_image, remove_tag_from_image
        │   ├── notes.rs        # get_image_notes, set_image_notes
        │   ├── roots.rs        # get_scan_root, set_scan_root, list_roots, add_root, remove_root, set_root_enabled
        │   ├── similarity.rs   # get_similar_images, get_tiered_similar_images, get_fused_similar_images (Phase 5 RRF)
        │   ├── semantic.rs     # semantic_search
        │   └── profiling.rs    # is_profiling_enabled, get_perf_snapshot, reset_perf_stats, export_perf_snapshot, record_user_action
        ├── db/                 # SQLite layer (post-split — was 1.6k-line db.rs)
        │   ├── mod.rs          # ImageDatabase struct + WAL/NORMAL pragma + foreign_keys=ON + CREATE TABLE flow
        │   ├── schema_migrations.rs  # 3 idempotent ALTER TABLE migrations (thumbnails, multifolder, notes/orphaned)
        │   ├── images_query.rs # aggregate_image_rows helper + get_images*, get_paths_to_root_ids, get_pipeline_stats, AND/OR tag SQL
        │   ├── embeddings.rs   # bytemuck::cast_slice (replaces 3 unsafe blocks); get_all_embeddings (single-SELECT)
        │   ├── tags.rs         # create/delete/get tags + add/remove join rows
        │   ├── thumbnails.rs   # update_image_thumbnail, get_image_thumbnail_info
        │   ├── roots.rs        # roots CRUD + migrate_legacy_scan_root + wipe_images_for_new_root
        │   ├── notes_orphans.rs# add_image, get/set notes, mark_orphaned (chunked UPDATE for SQLite param limit)
        │   └── test_helpers.rs # `fresh_db()` for the per-submodule test modules
        ├── filesystem.rs       # ImageScanner — recursive read_dir + 7-extension whitelist
        ├── thumbnail/
        │   ├── mod.rs          # pub use generator::ThumbnailGenerator
        │   └── generator.rs    # 400×400 max, aspect-preserving, JPEG; per-root subfolder layout
        ├── similarity_and_semantic_search/
        │   ├── mod.rs          # Re-exports the submodules
        │   ├── encoders.rs     # ImageEncoder + TextEncoder traits — runtime dispatch seam
        │   ├── encoder.rs      # ClipImageEncoder via ort; 224×224 bicubic-shortest-edge + center-crop,
        │   │                   # CLIP-native mean/std, separate vision_model.onnx, batch=32, L2-normalize
        │   ├── encoder_dinov2.rs   # Dinov2ImageEncoder; 224×224 bicubic-shortest-edge-256 + center-crop-224,
        │   │                       # ImageNet mean/std, CLS-token from last_hidden_state, 768-d output
        │   ├── encoder_siglip2.rs  # Siglip2ImageEncoder + Siglip2TextEncoder; 256×256 exact-square bilinear
        │   │                       # + [-1,1] for image; Gemma SP, 64 tokens, NO attention_mask for text;
        │   │                       # both branches use pooler_output (MAP head), 768-d shared space
        │   ├── ort_session.rs # Phase 2d/R4: shared M2-tuned `Session` builder. Level3 + intra_threads(4) +
        │   │                   # inter_threads(1). Every encoder constructor (CLIP image+text, DINOv2,
        │   │                   # SigLIP-2 image+text) goes through this so a future tuning change lands once.
        │   ├── cosine_similarity.rs  # 9-line shim: `pub use crate::similarity_and_semantic_search::cosine::*;`
        │   ├── cosine/         # Post-split (was 860-line cosine_similarity.rs)
        │   │   ├── mod.rs      # Module decls + pub use index::CosineIndex
        │   │   ├── math.rs     # cosine_similarity helper + score_cmp_desc (NaN-aware)
        │   │   ├── index.rs    # CosineIndex + populate_from_db_for_encoder + 3 retrieval modes
        │   │   │               # + scratch buffer + select_nth_unstable_by partial sort (2.53× speedup)
        │   │   │               # + emits cosine_cache_populated, embedding_stats,
        │   │   │               #   pairwise_distance_distribution, self_similarity_check diagnostics
        │   │   ├── rrf.rs      # Phase 5: Reciprocal Rank Fusion (Cormack 2009, k=60). Fuses N per-encoder
        │   │   │               # ranked lists into one. Powers get_fused_similar_images. 6 unit tests.
        │   │   ├── diagnostics.rs  # 4 stateless helpers: embedding_stats, pairwise_distance_distribution,
        │   │   │                   # self_similarity_check, score_distribution_stats
        │   │   └── cache.rs    # Persistent cosine_cache.bin (bincode); load_from_disk_if_fresh checks DB mtime
        │   └── encoder_text/   # Post-split (was 647-line encoder_text.rs)
        │       ├── mod.rs      # pub use ClipTextEncoder
        │       ├── encoder.rs  # ClipTextEncoder via HF tokenizers crate (BPE 49k, max 77 tokens,
        │       │               # pad with id 49407); ort session (CoreML disabled for transformer ops);
        │       │               # exposes tokenizer_for_diagnostic() for the tokenizer_output diagnostic
        │       └── pooling.rs  # normalize, try_extract_single_embedding, mean_pool (ort-free for testability)
        ├── indexing.rs         # Background pipeline (single-flight AtomicBool); 4 phases + cosine_repopulate; emits IndexingProgress events
        ├── watcher.rs          # notify-debouncer-mini start; rescan trigger via try_spawn_pipeline (single-flight coalesces bursts)
        ├── model_download.rs   # First-launch HuggingFace download (image, text, tokenizer); HEAD preflight + chunked GET + progress callback
        ├── settings.rs         # `Settings { scan_root: Option<PathBuf> }` — legacy single-folder pre-Phase-6
        ├── paths.rs            # Library/ layout helpers; dev branches on debug_assertions to <repo>/Library/, release uses dirs::data_dir()
        ├── perf.rs             # PerfLayer (tracing-subscriber Layer), per-span aggregate stats, RawEvent log, JSONL flush thread
        ├── perf_report.rs      # On-exit markdown report renderer + raw.json
        ├── image_struct.rs     # ImageData (id, path, tags, thumbnail_path?, w?, h?, notes?, orphaned)
        ├── tag_struct.rs       # Tag (id, name, color)
        └── root_struct.rs      # Root (id, path, enabled, added_at)
```

## Subsystem Responsibilities

```
                 ┌────────────────────────────────────────────────┐
                 │              React 19 Frontend                 │
                 │                                                │
   Browser ──►   pages/[...slug].tsx  (search-routing, hotkeys)    │
                       │   │   │                                   │
                       │   │   └─► PerfOverlay (--profile only)    │
                       │   │                                       │
                       ▼   ▼                                       │
                 Masonry / SearchBar / PinterestModal /            │
                 TagDropdown / IndexingStatusPill / settings/      │
                       │                                           │
                       ▼                                           │
                 useUserPreferences (localStorage) +               │
                 TanStack Query hooks (frontend-state) +           │
                 useIndexingProgress (Tauri event subscription)    │
                       │                                           │
                       │ services/* → invoke() / event listen      │
                  ─────┼──────── Tauri IPC + event boundary ───────┤
                       │                                           │
                       ▼ (typed: ApiError on the wire)             │
                  ┌─────────────────────────────────────────────┐  │
                  │             Rust Backend                    │  │
                  │                                             │  │
                  │  lib.rs::run — manage state + setup +       │  │
                  │     invoke_handler![26 commands]            │  │
                  │     │                                       │  │
                  │     ├─► commands/  (per-concern)            │  │
                  │     │      └─► db/  (WAL+NORMAL SQLite)      │ │
                  │     │      └─► cosine/index + encoder_text   │ │
                  │     │      └─► perf::record_user_action       │ │
                  │     │                                       │  │
                  │     ├─► indexing.rs (background thread)     │  │
                  │     │      └─► model_download → scan →      │  │
                  │     │          orphan-mark → thumbnail (rayon)│ │
                  │     │          → encode → cosine populate +  │ │
                  │     │          save_to_disk → Phase::Ready  │  │
                  │     │     emits indexing-progress every step │  │
                  │     │                                       │  │
                  │     └─► watcher.rs (notify-debouncer-mini)  │  │
                  │            └─► every 5s debounce → re-spawn  │ │
                  │                indexing pipeline (single-    │ │
                  │                flight coalesces)             │ │
                  │                                             │  │
                  │  perf.rs (only mounted on --profile):       │  │
                  │     PerfLayer aggregates spans               │ │
                  │     + JSONL timeline + on-exit report.md    │  │
                  └─────────────────────────────────────────────┘  │
                                                                   │
                  ─── disk: <repo>/Library/ in dev ────────────────┘
                       images.db (WAL), settings.json,
                       cosine_cache.bin, models/*.onnx,
                       thumbnails/root_<id>/thumb_<id>.jpg,
                       exports/perf-<unix_ts>/{timeline.jsonl,
                                              report.md, raw.json}
```

| System | Owns | Source location | Canonical doc |
|--------|------|-----------------|---------------|
| `database` | SQLite schema, 5 tables, WAL+NORMAL pragmas, embedding BLOB encoding (bytemuck), idempotent migrations, AND/OR tag filter SQL, orphan filter, pipeline stats | `src-tauri/src/db/` | `systems/database.md` |
| `tauri-commands` | 22-command IPC surface, `ApiError` typed-error wire format, `ImageSearchResult` unified shape, lazy text-encoder init, path-prefix normalisation | `src-tauri/src/commands/` | `systems/tauri-commands.md` |
| `indexing` | Background pipeline (scan → model-download → orphan-mark → thumbnail → encode → cosine-repopulate), single-flight AtomicBool, IndexingProgress events | `src-tauri/src/indexing.rs` | `systems/indexing.md` |
| `watcher` | `notify-debouncer-mini` recursive watch on every enabled root; 5s debounce → re-spawn pipeline | `src-tauri/src/watcher.rs` | `systems/watcher.md` |
| `multi-folder-roots` | `roots` table CRUD, enabled toggle, `set_scan_root` vs `add_root` semantics, `migrate_legacy_scan_root`, per-root thumbnail directories | `src-tauri/src/db/roots.rs`, `commands/roots.rs`, `paths::thumbnails_dir_for_root` | `systems/multi-folder-roots.md` |
| `model-download` | First-launch HuggingFace fetch for `model_image.onnx`, `model_text.onnx`, `tokenizer.json`; HEAD preflight + chunked GET + per-byte progress | `src-tauri/src/model_download.rs` | `systems/model-download.md` |
| `paths-and-state` | `Library/` directory layout, dev-vs-release branching, settings.json, cosine_cache.bin, exports/, `strip_windows_extended_prefix` | `src-tauri/src/paths.rs`, `settings.rs` | `systems/paths-and-state.md` |
| `profiling` | `--profile` flag, `PerfLayer` span aggregation, RawEvent log, JSONL flush, on-exit `report.md` renderer, frontend `<PerfOverlay>` + `perfInvoke` + action breadcrumbs | `src-tauri/src/perf.rs`, `perf_report.rs`, `src/components/PerfOverlay.tsx`, `src/services/perf.ts` | `systems/profiling.md` |
| `filesystem-scanner` | Recursive image discovery, 7-extension whitelist (read by indexing) | `src-tauri/src/filesystem.rs` | `systems/filesystem-scanner.md` |
| `thumbnail-pipeline` | Aspect-preserving 400×400 cached thumbnails on disk; per-root subfolder layout; rayon-parallel | `src-tauri/src/thumbnail/generator.rs` | `systems/thumbnail-pipeline.md` |
| `clip-image-encoder` | OpenAI CLIP ViT-B/32 separate `vision_model.onnx`; bicubic-shortest-edge-224 + center-crop, CLIP-native mean/std, 512-d L2-normalised, batched | `src-tauri/src/similarity_and_semantic_search/encoder.rs` | `systems/clip-image-encoder.md` |
| `clip-text-encoder` | OpenAI English CLIP separate `text_model.onnx`; HF `tokenizers` crate BPE (max 77 tokens, pad id 49407), 512-d L2-normalised, lazy + pre-warm init | `similarity_and_semantic_search/encoder_text/` | `systems/clip-text-encoder.md` |
| `dinov2-encoder` | DINOv2-Base (Meta self-supervised); image-only; bicubic-shortest-edge-256 + center-crop-224, ImageNet mean/std, CLS-token from `last_hidden_state[:,0,:]`, 768-d L2-normalised | `src-tauri/src/similarity_and_semantic_search/encoder_dinov2.rs` | `systems/dinov2-encoder.md` |
| `siglip2-encoder` | SigLIP-2 Base 256 (Google sigmoid loss); image+text in shared 768-d space; image: 256×256 exact-square bilinear + [-1,1]; text: Gemma SP 64 tokens NO attention_mask; both use `pooler_output` (MAP head). **Text-branch picker dispatch landed Phase 4, 2026-04-26**. | `src-tauri/src/similarity_and_semantic_search/encoder_siglip2.rs` | `systems/siglip2-encoder.md` |
| `cosine-similarity` | In-memory similarity index, `select_nth_unstable_by` partial-sort (2.53× speedup), reusable scratch buffer, persistent disk cache | `similarity_and_semantic_search/cosine/` | `systems/cosine-similarity.md` |
| `multi-encoder-fusion` | **NEW (Phase 5)** — Reciprocal Rank Fusion (Cormack 2009, k=60) across CLIP + SigLIP-2 + DINOv2 for image-image similarity. Per-encoder cosine caches in `FusionIndexState`. Replaces tiered random-sampling. | `similarity_and_semantic_search/cosine/rrf.rs`, `commands/similarity.rs::get_fused_similar_images`, `lib.rs::FusionIndexState` | `systems/multi-encoder-fusion.md` |
| `masonry-layout` | Shortest-column packing, hero promotion, 3D tilt, sortMode-aware, dimensions sourced from backend (no DOM image-load round-trip) | `src/components/Masonry.tsx`, `MasonryItem.tsx`, `MasonryAnchor.tsx` | `systems/masonry-layout.md` |
| `tag-system` | Tag CRUD + delete (now wired), optimistic mutations, AND/OR filter mode toggle, `#` autocomplete, create-on-no-match | `src/components/{SearchBar,TagDropdown}.tsx`, `useTags.ts`, `useImages.ts` | `systems/tag-system.md` |
| `search-routing` | Frontend priority chain: similar > semantic > tag > all; debounced semantic; selectedItem now resolved against `displayImages` (audit fix) | `src/pages/[...slug].tsx` | `systems/search-routing.md` |
| `frontend-state` | TanStack Query config, settings/ subdirectory, `useUserPreferences` localStorage layer, `useIndexingProgress` event hook, `useRoots` mutations | `src/queries/`, `src/hooks/`, `src/components/settings/` | `systems/frontend-state.md` |

## Dependency Direction

```
        main.rs (binary entry)
            │ parse --profile, init tracing + opt-in PerfLayer,
            │ open SQLite + initialize (WAL/NORMAL/foreign_keys)
            ▼
    db ◄─────────────────────────────────────────────────┐
     ▲                                                    │
     │                                                    │
   filesystem ────► (called by indexing)                  │
   thumbnail ─────► (called by indexing) ─reads/writes──► │
   image-encoder ─► (called by indexing) ─writes──────►   │
   text-encoder ──► (called by commands::semantic) ─reads►│
   cosine/index ──► (called by commands::similarity +     │
                    semantic, populated by indexing) ────►│
   model-download► (called by indexing)                   │
   paths ─────────► (called by everything reading state)  │
   settings ──────► (called by lib.rs setup for legacy migration)
   perf ──────────► (mounted only when --profile set)     │
   indexing ──────► spawns thread, calls every encoder/   │
                    thumbnail/db/cosine module in order,  │
                    holds Arc<Mutex<CosineIndex>> + Arc<  │
                    IndexingState> + AppHandle for events │
   watcher ───────► triggers indexing::try_spawn_pipeline │
                    on debounced filesystem events         │
                                                          │
                  lib.rs::run ◄──────────────────────────┘
                       │ tauri::Builder.manage(db, cosine_state,
                       │   text_encoder_state, indexing_state, watcher_state)
                       │ .setup(legacy migrate + spawn pipeline + start watcher)
                       │ .invoke_handler![26 commands]
                       │ .run(|_,e| if Exit && profiling { render_session_report })
                       ▼
                  Frontend (services → queries → components)
```

Key directional rules observed in code:

- **`db` is the only sink** every backend module writes to or reads from. It has no inverse dependencies.
- **`cosine::populate_from_db` now takes `&ImageDatabase`** (audit finding `ae0006d`/`5c2b0f6`) and uses the single-SELECT `get_all_embeddings` — no second connection, no per-row lookup.
- **`Arc<Mutex<CosineIndex>>` is intentionally cloned across thread boundaries** — the indexing thread (background) and the Tauri-managed `CosineIndexState` (foreground commands) both hold clones pointing at the same in-memory cache. This is what lets a finished pipeline-encode immediately make new embeddings available to the next semantic search.
- **`indexing.rs` and `watcher.rs` are coupled through `IndexingState` (single-flight AtomicBool)** — rapid filesystem events that try to spawn a second pipeline get `Err(AlreadyRunning)` back and silently coalesce.
- **`profiling` is not in the normal data path.** When `--profile` is absent, `PerfLayer` never registers, all `#[tracing::instrument]` overhead reduces to one tracing dispatch per call (the env filter passes the spans but no aggregator builds them), the frontend `PerfOverlay` never mounts, and `record_user_action` is a no-op. All profiling-related code paths stay cold.
- **`commands` returns `Result<T, ApiError>`** for every handler. The frontend deserialises `{ kind, details }` and branches on kind in `formatApiError`. Strings on the wire still parse via the legacy fallback.
- **Frontend services never call `invoke()` directly** — they wrap it in functions that translate Tauri JSON into UI types. Hooks call services; components call hooks.

## Core Execution / Data Flow

### Startup pipeline (`main.rs` → `lib.rs::run` → `indexing.rs::run_pipeline_inner`)

```
1. Parse --profile flag                            main.rs:22
2. Init tracing subscriber (PerfLayer only on --profile) main.rs:48-79
3. Open SQLite handle + initialize                 main.rs:89-91, db/mod.rs:47-145
   • PRAGMA journal_mode=WAL
   • PRAGMA synchronous=NORMAL
   • PRAGMA foreign_keys=ON
   • CREATE TABLE roots / images / tags / images_tags
   • Run 3 migrations (thumbnails, multifolder, notes/orphaned)
4. Hand to image_browser_lib::run(db, db_path)     main.rs:93
5. tauri::Builder .manage(...).setup(|app| {       lib.rs:82-170
   5a. Legacy migration: settings.json::scan_root → roots row
   5b. indexing::try_spawn_pipeline(...)  ← background thread
   5c. watcher::start(every enabled root, recursive)
}).invoke_handler![26 commands].build().run(|e| if Exit && profiling { render_session_report })

Background pipeline (indexing.rs::run_pipeline_inner) runs while UI is interactive:
  i.    Try to load cosine_cache.bin                   indexing.rs:182-189; cosine/cache.rs
  ii.   model_download::download_models_if_missing     indexing.rs:217 (HEAD preflight + chunked GET + progress)
  iii.  Pre-warm text encoder (ONNX session + tokenizer) indexing.rs:232-259
  iv.   Open second ImageDatabase (rusqlite supports concurrent connections; WAL keeps reads non-blocking)
  v.    Phase::Scan: walk every enabled root, INSERT OR IGNORE, mark_orphaned per root
  vi.   Phase::Thumbnail: rayon par_iter, single-SELECT path_to_root_ids, write to thumbnails/root_<id>/
  vii.  Phase::Encode: batches of 32, CLIP image encoder, write embedding BLOB
  viii. cosine::populate_from_db (single SELECT) + cosine::save_to_disk
  ix.   Phase::Ready emitted with final image count
```

Each phase emits an `indexing-progress` Tauri event so the frontend `IndexingStatusPill` renders a live status. Steps v, vi, vii are idempotent — re-launches on a populated DB are fast because `INSERT OR IGNORE` skips known paths and `get_images_without_*` only returns rows missing the relevant artefact.

### Runtime: image grid load

```
Frontend                              Backend
──────                                ──────
useImages({tagIds, searchText, matchAllTags, sortMode, shuffleSeed})
  └─► fetchImages(tagIds, searchText, matchAllTags)
        └─► invoke("get_images")
                                       db.get_images_with_thumbnails(tagIds, "", matchAllTags)
                                         └─► aggregate_image_rows helper (audit finding)
                                         └─► WHERE root enabled AND NOT orphaned
                                         └─► AND/OR semantic via match_all_tags
                                         └─► Stable sort by id (was random shuffle pre-Phase-9)
        ◄─── Vec<ImageData> (typed wire: Result<_, ApiError>)
  └─► map to ImageItem with convertFileSrc(thumbnail_path)
TanStack Query caches by ["images", tagIds, searchText, matchAllTags]
Frontend may apply session-seeded shuffle if sortMode === "shuffle"
Masonry receives items; computes shortest-column packing using backend-supplied (w, h)
```

### Runtime: semantic search end-to-end (chosen Dependency Chain Trace)

This is the obligation's chosen critical path because it crosses the most boundaries (UI, debounce, IPC, mutex, ONNX, mutex, math, mutex, DB, IPC, render):

```
[1] User types into SearchBar → useState(searchText)
        ──► useDebouncedValue(searchText, 300)               src/pages/[...slug].tsx:84
[2] shouldUseSemanticSearch test (non-empty, no #, no selection) pages/[...slug].tsx:90
[3] useSemanticSearch(query, prefs.semanticResultCount) → useQuery
        queryKey ["semantic-search", query, top_n]; staleTime 5min, gcTime 10min
[4] semanticSearch(query, top_n)
        invoke("semantic_search", { query, topN })           services/images.ts
        ─── Tauri IPC boundary ───
[5] commands::semantic::semantic_search:                     commands/semantic.rs
      • text_encoder_state.encoder.lock()  → ApiError::Cosine on poison
      • Lazy init (RARE — pre-warmed in pipeline):
          if !clip_text.onnx exists ⇒ ApiError::TextModelMissing(path)   ← TYPED, frontend can branch
          if !clip_tokenizer.json exists ⇒ ApiError::TokenizerMissing(path)
          ClipTextEncoder::new(model_path, tokenizer_path)   // HF tokenizers crate inside
      • Diagnostic emit BEFORE inference (no-op if profiling off):
          tokenizer_for_diagnostic().encode(query, true)
          → record_diagnostic("tokenizer_output", { raw_query, decoded_tokens, token_ids,
                                                     attention_mask_sum, max_seq_length=77,
                                                     interpretation: "OK" / "WARNING" / "ERROR" })
      • encoder.encode(query)                              encoder_text/encoder.rs
        └─► tokenizer: HF tokenizers BPE + RobertaProcessing → pad/truncate to 77 with id 49407
        └─► ort session.run(input_ids: int64[1,77], attention_mask: int64[1,77])
        └─► extract output (try text_embeds → pooler_output → sentence_embedding)
        └─► L2-normalize via super::pooling::normalize
        └─► returns Vec<f32> length 512
      • Always force CLIP cosine cache:
          cosine_state.ensure_loaded_for(&db, "clip_vit_b_32")  → ApiError::Cosine on poison
          (Triggers populate_from_db_for_encoder if cache holds a different encoder.
           That populate emits cosine_cache_populated + embedding_stats +
           pairwise_distance_distribution + self_similarity_check diagnostics.)
      • get_similar_images_sorted(query, top_n, None)
        └─► scratch.clear() + similarity for every cached image (no PathBuf clone in inner loop)
        └─► select_nth_unstable_by (partial sort) — 2.53× faster than full sort at n=10000
        └─► take top_n, sort the returned slice
      • Compute query L2 norm + range + NaN/Inf counts (for query_embedding diagnostic)
      • all_images = db.get_all_images().ok()  (cached once)
      • For each result path:
          resolve_image_id_for_cosine_path(&db, &path, all_images.as_deref())
            └─► strip \\?\ prefix via paths::strip_windows_extended_prefix (Cow, no alloc on common path)
            └─► db.get_image_id_by_path(normalised) → Some((id, normalised))
            └─► fallback: same lookup against raw path
            └─► fallback: walk all_images comparing canonical forms
        Resolution misses are tracked into a `resolution_misses` Vec<String> for the diagnostic.
      • For each resolved id: db.get_image_thumbnail_info → enrich ImageSearchResult
        Thumbnail-info misses tracked into thumb_misses counter.
      • record_diagnostic("search_query", {
          type: "semantic", encoder_id: "clip_vit_b_32", top_n, query_text, cosine_cache_size,
          query_embedding: { dim, l2_norm, min, max, nan_count, inf_count, interpretation },
          raw_results: [{path, score}, ...],
          score_distribution: cosine::diagnostics::score_distribution_stats(&raw_scores),
          path_resolution_outcomes: { raw_count, resolved_count, missed_count,
                                       thumbnail_misses, missed_paths_sample }
        })
        ─── Tauri IPC return ───
[6] services/images.ts catch site uses formatApiError(e); typed kinds get specific UI affordances
[7] pages/[...slug].tsx::displayImages branch fires semantic; Masonry renders sorted result list
```

**Boundary failure semantics for this chain:**

| Step | Failure | Behaviour |
|------|---------|-----------|
| [1]-[3] | Empty query | `enabled` flag is false; query never runs |
| [3] | Hash-prefixed query | `shouldUseSemanticSearch` is false; routes through tag filter |
| [5] mutex lock | Mutex poisoned | `From<PoisonError> for ApiError` ⇒ `ApiError::Cosine("mutex poisoned: ...")` returned via `?` |
| [5] lazy init | model_text.onnx missing | `ApiError::TextModelMissing(path)` — frontend can call `isMissingModelError` and trigger re-download flow |
| [5] tokenizer.json missing | Same — `ApiError::TokenizerMissing(path)` |
| [5] ONNX run | Session error | `ApiError::Encoder("encode query: ...")` |
| [5] populate_from_db | DB read error | `populate_from_db` swallows the error and logs (cache stays empty); the next `get_similar_images_sorted` returns Vec::new(). Non-fatal. |
| [5] DB id resolve | None of 3 strategies match | Silently filtered out — user gets fewer results, no error |
| [6]-[7] | Mutation fails | Optimistic update rolled back via `onError` snapshot restore (tag mutations only) |

The chain crosses 4 process boundaries (UI → IPC → DB → ONNX) and 2 synchronisation boundaries (TextEncoder mutex, CosineIndex mutex). Both mutexes are held for the duration of the operation — concurrent semantic searches serialise. The DB mutex is acquired briefly for `get_all_images` and `get_image_thumbnail_info` calls under WAL, so foreground reads don't block the background indexing thread's writes.

## Inter-System Relationships

This table satisfies the inter-system relationship mapping obligation. With 19 system files the floor is `min(19, C(19,2)=171) = 19`. Entries 26-30 added 2026-04-26 by the Tier 1 + Tier 2 + Phase 4 + Phase 5 perf bundle (see `plans/perf-optimisation-plan.md`).

| # | A | B | Mechanism | What breaks if it fails |
|---|---|---|-----------|-------------------------|
| 1 | filesystem-scanner | indexing | `ImageScanner::scan_directory(&Path) -> Result<Vec<String>>` called per enabled root inside the pipeline | A scan with a permission error logs warn and the pipeline continues with whatever was collected before — partial scans are OK because `INSERT OR IGNORE` is idempotent on retry. |
| 2 | indexing | database | Pipeline drives `add_image`, `get_images_without_thumbnails`, `update_image_thumbnail`, `get_images_without_embeddings`, `update_image_embedding`, `mark_orphaned`. `get_paths_to_root_ids` is single-SELECT (audit `0bdb5f4`). | If the DB Mutex contends, the pipeline blocks. WAL means foreground reads still work; foreground writes serialise. |
| 3 | indexing | thumbnail-pipeline | Pipeline holds a `ThumbnailGenerator` and rayon-pars the `needs_thumbs` list, looking up each image's `root_id` from the pre-fetched map | A thumbnail decode failure logs and the row stays unmarked — next pipeline run retries it. |
| 4 | indexing | clip-image-encoder | Pipeline `run_clip_encoder` instantiates `ClipImageEncoder::new(clip_vision.onnx)` and batches via `encode_batch(&[Path])` (size 32). Writes BOTH the legacy `images.embedding` BLOB and the per-encoder `embeddings(image_id, "clip_vit_b_32", ...)` row per image. | If `clip_vision.onnx` is missing the encode phase is skipped (`warn` logged). Non-fatal — semantic search just returns fewer results. |
| 5 | indexing | clip-text-encoder | Pipeline pre-warms the text encoder (`ClipTextEncoder::new(clip_text.onnx, clip_tokenizer.json)` and stows in `TextEncoderState` Mutex) so the first user-visible semantic search doesn't pay 1-2 s of model-load time | If pre-warm fails (logged warn), the lazy init path in `commands::semantic` covers it on first use. |
| 6 | indexing | cosine-similarity | Pipeline calls `cosine.populate_from_db(&ImageDatabase)` then `cosine.save_to_disk()` after every encode pass; `Arc<Mutex<CosineIndex>>` is shared between indexing thread + Tauri-managed `CosineIndexState`. Per-encoder cache loads happen via `ensure_loaded_for(&db, encoder_id)` from command handlers. | If the cosine cache file is corrupted, `load_from_disk_if_fresh` silently skips and the next populate fully rebuilds. |
| 6a | indexing | dinov2-encoder | Pipeline `run_trait_encoder("dinov2_base", Dinov2ImageEncoder::new)` runs after CLIP, encodes images that lack a `dinov2_base` row in the embeddings table. ImageNet preprocessing distinct from CLIP's. | If model file missing, that encoder's pass skips with `warn`; other encoders unaffected. Per-encoder fail-soft. |
| 6b | indexing | siglip2-encoder (image branch) | Pipeline `run_trait_encoder("siglip2_base", Siglip2ImageEncoder::new)` runs after DINOv2. 256×256 exact-square preprocessing distinct from CLIP/DINOv2. | Same fail-soft behaviour. |
| 6c | siglip2-encoder (text branch) | commands::semantic | NOT YET WIRED to the picker dispatch — `commands::semantic::semantic_search` still hardcodes `ClipTextEncoder`. The picker UI shows an "experimental" warning. | When wired: text query → SigLIP-2 text encoder → cosine against the SigLIP-2 image-cache namespace. |
| 7 | indexing | watcher | Both share `Arc<IndexingState>` (single-flight `AtomicBool`). Watcher debounce-callback calls `try_spawn_pipeline` which returns `Err(AlreadyRunning)` if a run is in flight — second event is silently coalesced. | If single-flight breaks, two pipelines could run concurrently, double-writing the same paths. WAL + `INSERT OR IGNORE` makes this safe but wastes CPU. |
| 8 | indexing | model-download | Pipeline phase 1 calls `download_models_if_missing(progress_cb)`; missing files fetched from HuggingFace with HEAD preflight + chunked GET; progress flows back via callback into `Phase::ModelDownload` events | Network failure logs `warn` and continues with whatever models exist. Encode/text-encoder phases gate on `path.exists()`. |
| 9 | watcher | tauri::AppHandle | Watcher closure captures `app.clone()` so it can call `try_spawn_pipeline` and emit `indexing-progress`. Handle stashed in `Arc<Mutex<Option<WatcherHandle>>>` so dropping it cancels every watch. | Dropping the handle (e.g., recreating watcher on root change) cancels active watches — currently the watcher is NOT rebuilt on `add_root`/`remove_root`, so new roots aren't watched until next launch. Documented gap. |
| 10 | multi-folder-roots | thumbnail-pipeline | Each thumbnail lands in `paths::thumbnails_dir_for_root(root_id)`; `remove_root` `rm -rf`s the per-root subfolder (best-effort, warn-on-fail) | Without per-root layout, root removal would orphan thumbnail files forever. Legacy rows with `root_id = NULL` still write to the flat layout. |
| 11 | multi-folder-roots | database | `roots` table; `images.root_id INTEGER REFERENCES roots(id) ON DELETE CASCADE`; `PRAGMA foreign_keys=ON` was the explicit fix that made CASCADE actually fire | Disabling FK pragma silently turns CASCADE into a no-op — orphan image rows accumulate. |
| 12 | tauri-commands | ApiError + frontend apiError.ts | Wire format pinned by `#[serde(tag="kind", content="details")]`. Frontend `ApiError` discriminated union mirrors the kinds; `formatApiError(unknown)` covers ApiError + legacy strings + Error instances | Adding a backend variant without updating the TS union triggers no runtime error — the default case in `formatApiError` handles unknown kinds gracefully. |
| 13 | tauri-commands | cosine-similarity + clip-text-encoder | `commands::similarity` and `commands::semantic` lock `CosineIndexState.index` (Mutex) and `TextEncoderState.encoder` (Mutex<Option<...>>) via `?` (uses `From<PoisonError>` impl) | Mutex poison surfaces as `ApiError::Cosine`; user can retry — the next call relocks fresh. |
| 14 | profiling | every instrumented system | `tracing::info_span!` and `#[tracing::instrument]` on commands, indexing phases, model_download, watcher events, cosine retrievals. Spans collected by `PerfLayer` only when `--profile` is set. | Without `--profile`, span construction still happens (env filter passes) but the aggregator never registers — overhead is one tracing dispatch per call, no allocation. |
| 15 | profiling | frontend overlay + perfInvoke | `is_profiling_enabled` command resolved at mount; `<PerfOverlay>` mounts only if true; `recordAction` calls into `record_user_action` which appends to the timeline only when profiling is on; `perfInvoke` wraps Tauri `invoke` with a `React.Profiler`-style start/end emit | Without profiling, all React profiling-related state is dead — `useState(profiling)` is `false` and every gated branch short-circuits. |
| 16 | search-routing | tauri-commands | `pages/[...slug].tsx` invokes `get_images`, `get_tiered_similar_images`, `semantic_search` and chooses outputs by priority (similar > semantic > tag > all). Selection lookup now uses `displayImages` not `images.data` (audit fix `9d04f69`) | If a backend command's JSON shape changes without a TS update, the priority union silently falls back to whichever branch did succeed. |
| 17 | masonry-layout | search-routing + database | Masonry receives `displayImages: ImageItem[]` from routing, plus `selectedItem` for hero promotion. Tile dimensions now come from the DB row (audit `fb23bdb` — was DOM-Image round-trip). | If `width/height` are NULL (legacy un-thumbnailed rows), Masonry falls back to a default aspect ratio. |
| 18 | tag-system | database | Tags use `tags` and `images_tags` tables; `add_tag_to_image` is `INSERT OR IGNORE` (Phase 6 hardening). `delete_tag` is now wired through `commands::tags::delete_tag` (audit Phase 6). AND/OR controlled by `match_all_tags: bool` parameter on `get_images_with_thumbnails`. | A typo'd tag now has a UI delete affordance; before this commit it accumulated forever. |
| 19 | frontend-state | search-routing + tag-system + masonry + indexing | `queryClient.ts` sets `staleTime: Infinity` — caches never auto-stale. `useIndexingProgress` fires `invalidateQueries(["images"])` on `Phase::Ready`. Tag mutations use `invalidateQueries` after success. | A stale cache after a tag mutation would show wrong tags; `onSuccess` invalidation handles this. Cross-cache-key staleness still possible if a future feature mutates state without invalidating. |
| 20 | paths-and-state | database + thumbnail + cosine + model-download + perf + settings | `paths::*_dir()` helpers are the single source for every disk path. Dev branches on `cfg(debug_assertions)` to `<repo>/Library/`; release falls back to `dirs::data_dir()/com.ataca.image-browser/`. | If `paths::app_data_dir()` returns the wrong directory (e.g., a future macOS sandbox change), every state file goes to the wrong place. The dev build's `unwrap_or_else(PathBuf::from("."))` makes silent failures more likely. |
| 21 | settings (legacy) | multi-folder-roots | One-shot: lib.rs setup callback reads `settings.json::scan_root`; if present, calls `db.migrate_legacy_scan_root(path)` and clears the field so it doesn't re-migrate | If the user manually edits settings.json after migration, the legacy field could re-trigger; `migrate_legacy_scan_root` is idempotent (existing path → no-op). |
| 22 | profiling | every encoder + indexing + cosine + commands | `record_diagnostic(name, payload)` writes a `RawEvent::Diagnostic` to the perf log. 17 call sites across `commands/` (4), `indexing.rs` (3 — 2 in encoder loops, 1 in run summaries), `lib.rs` (2 — startup_state + cosine_math_sanity), `cosine/index.rs` (4 — cache_populated + 3 quality stats). When `--profile` is absent, every call is a no-op. | If `record_diagnostic` panicked, every encoder/cosine/command site would propagate. The body uses no `?` operators and no allocs that can panic, so the no-op-when-disabled fast path is hot. |
| 23 | cosine-similarity diagnostics | profiling | The 4 stateless helpers in `cosine/diagnostics.rs` (`embedding_stats`, `pairwise_distance_distribution`, `self_similarity_check`, `score_distribution_stats`) compute domain-specific stats and return `serde_json::Value` payloads. They are called from `cosine/index.rs::populate_from_db_for_encoder` and from `commands::similarity` / `commands::semantic` to enrich the `search_query` diagnostic. | If a helper panics on a malformed cache (NaN-only embeddings, empty cache), the encoder's populate pass would still succeed but the diagnostic payload would be missing. Today the helpers handle empty cases with explicit early returns. |
| 24 | embedding-pipeline migration | every encoder + cosine cache | `db/schema_migrations.rs::migrate_embedding_pipeline_version` runs once after the embeddings table is created. When `meta.embedding_pipeline_version < CURRENT_PIPELINE_VERSION`, wipes legacy CLIP + dinov2_small embeddings so the next indexing pass re-encodes under the new pipelines. SigLIP-2 rows aren't wiped (no prior data). | If the version constant is bumped without code-side preprocessing changes, embeddings are wiped and re-encoded with no quality change — wasteful but not broken. Bump must be paired with a real pipeline change. |
| 25 | tauri-commands | encoders.rs | `list_available_encoders` Tauri command serves the static `ENCODERS: &[EncoderInfo]` list (3 entries: clip_vit_b_32, siglip2_base, dinov2_base) to the frontend EncoderSection picker. Each entry carries id, display_name, description, dim, supports_text, supports_image. | EncoderInfo struct is mirrored as a TS interface in `EncoderSection.tsx`. Adding a backend entry without updating the picker UI's option-rendering logic surfaces as the option being available but with empty rationale text. |
| 26 | database (writer) | database (read-only secondary) | `ImageDatabase.connection: Mutex<Connection>` (writer) + `ImageDatabase.reader: OnceLock<Mutex<Connection>>` (read-only secondary, opened in `initialize()` on the same SQLite file with `SQLITE_OPEN_READ_ONLY`). Foreground SELECTs go through `read_lock()`; encoder writes use the writer. Both share the file's WAL — writes are serialised at the WAL layer, reads are non-blocking against active writes. | If the secondary fails to open (permission, missing file post-init), `read_lock` falls back to the writer mutex — restores the old contended-but-correct behaviour. `:memory:` test DBs deliberately have no secondary; writer covers reads in tests. |
| 27 | indexing | database (encoder write batching) | Both encoder loops in `indexing.rs` (`run_clip_encoder`, `run_trait_encoder`) call `database.upsert_embeddings_batch(encoder_id, &rows, legacy_clip_too)` once per ~32-image chunk instead of per-row `upsert_embedding`. Each batch is one `BEGIN IMMEDIATE` transaction → one COMMIT → one fsync. `database.checkpoint_passive()` runs between batches under `wal_autocheckpoint=0`. | If the batch fails partway, the transaction rolls back — no embeddings from that batch land. The encoder loop records each row as a write failure rather than pretending some succeeded. Rare under normal operation; mostly defends against disk-full / WAL-corruption edges. |
| 28 | cosine-similarity | FusionIndexState (Phase 5) | `FusionIndexState.per_encoder: Arc<Mutex<HashMap<String, CosineIndex>>>` holds one cosine cache per encoder family for image-image rank fusion. Lazy-populated on first `get_fused_similar_images` call per encoder; ~6 MB per encoder for 2000 images. `invalidate_all()` clears every slot — wired into `set_scan_root`, `remove_root`, `set_root_enabled` next to the existing `CosineIndexState.invalidate()` call. | If `invalidate_all` is forgotten on a future root-mutation IPC, fusion would return stale entries from the now-disabled root. The 3 existing call sites are paired so a code reviewer sees the pattern. |
| 29 | search-routing | FusionIndexState + RRF | `useTieredSimilarImages` hook now routes through `fetchFusedSimilarImages` → `get_fused_similar_images` IPC → `FusionIndexState.ranked_for_encoder` × 3 encoders → `cosine::rrf::reciprocal_rank_fusion(lists, k=60, top_n)` → resolved `ImageSearchResult[]`. Replaces the previous tiered random-sampling path (which still exists at `cosine/index.rs::get_tiered_similar_images` for reference but is no longer called from the frontend). | If a user clicks "View Similar" before any encoder has indexed the clicked image, the per-encoder loop emits "no_embedding_for_query_image" diagnostic entries and fusion returns empty rather than crashing. |
| 30 | search-routing | semantic.rs SigLIP-2 dispatch (Phase 4) | `useSemanticSearch` hook reads `prefs.textEncoder` and threads it through `semanticSearch(query, topN, textEncoderId)` → `semantic_search` IPC → branches on `text_encoder_id`: `Some("siglip2_base")` → `Siglip2TextEncoder` + SigLIP-2 cosine cache; otherwise CLIP. Both branches load the matching image-side cosine cache via `ensure_loaded_for` so dimensions match. | If the user picks SigLIP-2 in the picker before any SigLIP-2 image embeddings exist, semantic search returns 0 results (encoder produces a 768-d vector but the cache populate finds 0 rows) — the `cosine_cache_populated` diagnostic surfaces this as `count=0`. |

30 entries documented; floor of 19 cleared. Obligation cleared.

## Critical Paths and Blast Radius

The semantic-search end-to-end chain is the longest. Adjacent critical paths share most of its segments:

| Operation | Chain length | Shared with semantic? | Unique segment |
|-----------|--------------|----------------------|----------------|
| Tiered similar (image clicked) | UI → IPC → DB → CosineIndex (tiered sampling) → DB (id resolve + thumbnail info) → IPC → UI | yes (cosine + DB) | 7-tier within-tier sampling (`cosine/index.rs` `get_tiered_similar_images`) |
| Tag filter image load | UI → IPC → DB (LEFT JOIN with EXISTS-IN or GROUP BY HAVING for AND) → IPC → UI | partial (DB only) | AND vs OR SQL branch (`db/images_query.rs::get_images_with_thumbnails`) |
| Tag mutation | UI → optimistic UI → IPC → DB → IPC → invalidateQueries / rollback | partial (DB only) | TanStack Query rollback dance |
| Background indexing | indexing thread → model_download (HTTP) → text encoder pre-warm → DB scan → DB orphan-mark → rayon thumbnail → CLIP encode batches → cosine populate + save_to_disk | shares cosine + DB serialisation | Single-flight via AtomicBool; events emitted per phase |
| Filesystem watcher rescan | notify event → 5s debounce → try_spawn_pipeline → (single-flight may decline) → indexing pipeline | shares indexing + DB | Coalescing of bursts via single-flight |

All chains share the **DB connection mutexes** (foreground via `ImageDatabase`, background via the indexing thread's separate connection). WAL keeps reads non-blocking under the writer; the foreground reads through its mutex still serialise across foreground commands.

The **CosineIndex Arc<Mutex<...>>** is the second serialisation point — every similarity-driven operation queues, *and* the indexing pipeline's cosine-repopulate phase contends with active queries. In practice the contention is bounded because the indexing thread holds the lock briefly (one populate + one save) while queries hold it for the full sort + sample.

The **TextEncoder Mutex<Option<...>>** serialises every concurrent semantic search (rare). Lazy init was preserved even after pre-warm because pre-warm can fail (model file missing during indexing) and the lazy path still works on first user query.

## State Ownership

| State | Owner | Sharing pattern | Risk |
|-------|-------|-----------------|------|
| `Library/images.db` | `ImageDatabase` instance held by Tauri State (foreground) + a second `ImageDatabase` constructed inside the indexing thread (background) | Two connections to the same WAL'd file. WAL mode means foreground reads don't block background writes. | Without WAL the indexing pipeline would block UI reads for the duration of every batch encode. |
| `CosineIndex.cached_images` | `CosineIndexState.index: Arc<Mutex<CosineIndex>>` | Cloned across boundary: indexing thread + Tauri-managed state hold the same Arc. | The cache is invalidated on `set_scan_root` / `add_root` / `remove_root` / `set_root_enabled` (`commands::roots`) by clearing `cached_images` directly. Without that, root changes would leak old embeddings into queries. |
| `TextEncoder` | `TextEncoderState.encoder: Mutex<Option<TextEncoder>>` | Pre-warmed by indexing thread; lazy fallback in `commands::semantic`. | Heavy resource (ONNX session). Once loaded, lives until process exit. |
| `IndexingState.is_running` | `Arc<IndexingState>` (`AtomicBool`) | Tauri-managed + every command that triggers an index + watcher closure. RAII guard ensures the bool clears even if the pipeline panics. | If a command spawns a pipeline without the AtomicBool dance, two could run concurrently (would still be safe due to idempotent ops, but wasteful). |
| `WatcherHandle` slot | `Arc<Mutex<Option<WatcherHandle>>>` | Tauri-managed; setup callback fills it. Currently never refreshed when roots change. | Newly-added roots are not watched until the next launch. Documented gap. |
| `cosine_cache.bin` | `paths::cosine_cache_path()` (file on disk) | Written by `cosine.save_to_disk` after every successful encode pass; loaded in `try_spawn_pipeline` if fresher than DB | Stale cache (older than DB file) is silently skipped on load — no-op, falls through to populate_from_db. |
| `PROFILING_ENABLED` + `PERF_STATS` | Process-global `OnceLock`s in `perf.rs` | Set once in main; read everywhere | Setting twice is silently ignored (intentional). Reading before set returns the OnceLock's default. |
| Frontend image cache | TanStack QueryClient cache | Invalidated on indexing-progress `Phase::Ready`, on modal close (with shuffleSeed bump for the shuffle sortMode), and on tag mutations. staleTime is Infinity otherwise. | Tag-mutation rollback covers the common case; broader cross-key invalidation is manual. |
| `useUserPreferences` (theme, columns, sortMode, animation, search counts, tagFilterMode) | `localStorage["imageBrowserPrefs"]` + React state | Theme is also mirrored to `localStorage["theme"]` so `main.tsx` can apply it before React mounts (avoids FOUC) | localStorage may be disabled in some WebView modes — falls through to defaults. |
| `selectedItem`, `searchText`, `searchTags`, `settingsOpen`, `perfOpen` | React useState in `pages/[...slug].tsx` | Single render owner; URL slug is the source of truth for selection | Selection lookup now resolves against `displayImages` (audit fix `9d04f69`), so semantic-search-result clicks no longer fail silently. |

## Reading Guide

For a future session asking "where do I learn about X?":

| Goal | Read these in order |
|------|---------------------|
| Understand the whole repo from scratch | `architecture.md` → `notes.md` → `systems/database.md` → `systems/tauri-commands.md` → `systems/indexing.md` |
| Add a new Tauri command | `systems/tauri-commands.md` → `commands/error.rs` (ApiError patterns) → `notes/conventions.md` (lock pattern + From-impls) |
| Modify the indexing pipeline | `systems/indexing.md` → `systems/multi-folder-roots.md` → `systems/watcher.md` → `systems/cosine-similarity.md` (cache invalidation) |
| Add a new SQL table or column | `systems/database.md` § Migrations → `db/schema_migrations.rs` |
| Improve semantic search quality | `systems/clip-text-encoder.md` → `systems/siglip2-encoder.md` → `systems/cosine-similarity.md` → `notes/preprocessing-spatial-coverage.md` (open architectural concern about CLIP/DINOv2 center-crop dropping edge content) |
| Add or modify an encoder | The relevant `systems/{clip-image,clip-text,dinov2,siglip2}-encoder.md` → `systems/model-download.md` (URL/filename) → `commands/encoders.rs` (picker entry) → `db/schema_migrations.rs` (bump `CURRENT_PIPELINE_VERSION` if preprocessing changes invalidate prior data) |
| Read or extend the diagnostic system | `systems/profiling.md` § Domain diagnostics → `notes/conventions.md` § Domain diagnostics via `record_diagnostic` |
| Profile a performance regression | `systems/profiling.md` (run with `--profile`, read `Library/exports/perf-<unix_ts>/report.md`) |
| Wire a new frontend pref | `systems/frontend-state.md` → `src/hooks/useUserPreferences.ts` → relevant `settings/*Section.tsx` |

## Structural Notes / Current Reality

- **The repo is being driven hard.** Between 2026-04-25 and 2026-04-26, the project landed Phase 4 (folder picker), Phase 5 (async indexing pipeline), Phase 6 (CLIP quality + tracing + multi-folder), Phase 7 (filesystem watcher + orphans), Phase 9 (settings drawer + per-root thumbnails), Phase 11 (annotations + AND/OR tag filter), the entire profiling system, a 23-finding code-health audit (every finding shipped), the multi-encoder picker (Phases 1-3), and the encoder-pipeline overhaul (separate-graph CLIP, DINOv2-Base swap, SigLIP-2 wiring, 12-diagnostic system). Treat "active" sections of system docs as truly active.
- **The Tauri command count grew from 8 → 23.** The newest addition is `list_available_encoders` (serves the picker UI). Every command returns `Result<T, ApiError>` (except 3 profiling commands still on `Result<_, String>`).
- **Three encoder families are live.** CLIP ViT-B/32 (OpenAI English, 512-d, both branches), DINOv2-Base (Meta self-supervised, 768-d, image-only), SigLIP-2 Base 256 (Google sigmoid loss, 768-d shared image+text — but text-branch picker dispatch is partially wired). Each has its own preprocessing pipeline matching its training-time stats; mixing preprocessing across encoders silently degrades embedding quality with no error signal.
- **The `models/` directory is now auto-populated.** First launch downloads ~2.5 GB from HuggingFace with live progress; the Indexing pill renders MB/MB. Per-file fail-soft: a single 401/404 doesn't abort the batch.
- **Embedding-pipeline migration v2 wipes legacy CLIP + dinov2_small embeddings on first launch under the current code** — the next indexing pass re-encodes everything cleanly. Bump the version constant for any future preprocessing change that invalidates prior data.
- **The diagnostic system is the primary signal for encoder/search quality issues** — 12 named diagnostics including embedding L2-norm distribution, pairwise distance histogram, self-similarity sanity check, score distribution stats, tokenizer output, encoder run summary, preprocessing sample, cross-encoder comparison, cosine math sanity. All are no-ops without `--profile`. See `systems/profiling.md` § Domain diagnostics.
- **User state lives under `<repo>/Library/` in dev**, NOT in the platform app data dir. Release builds fall back to `~/Library/Application Support/com.ataca.image-browser/` on macOS via `dirs::data_dir()`. This is for project-local visibility — see `systems/paths-and-state.md`.
- **WAL is on.** Two SQLite connections coexist (foreground via `ImageDatabase`, background via the indexing thread's `ImageDatabase::new(db_path)`). Reads never block writes.
- **Three Tauri-managed `Mutex` singletons + two Arcs** serialise backend operations:
  - `Mutex<rusqlite::Connection>` inside `ImageDatabase` (per connection)
  - `Arc<Mutex<CosineIndex>>` shared between indexing thread + `CosineIndexState`
  - `Mutex<Option<TextEncoder>>` inside `TextEncoderState`
  - `Arc<IndexingState>` (`AtomicBool`) for single-flight
  - `Arc<Mutex<Option<WatcherHandle>>>` for the watcher slot
- **CoreML is enabled for the image encoder on macOS** but explicitly disabled for the text encoder — transformer ops are poorly supported by CoreML and produced runtime inference errors. CUDA is target-gated on non-macOS builds; both fall back to CPU.
- **The `assetProtocol` security scope is `["**"]`** with `csp: null`. Fine for a single-user local tool that only ever loads its own bundled HTML; flagged as a hardening target in `enhancements/recommendations/08-tauri-csp-asset-scope-hardening.md`.
- **Profiling is opt-in via `--profile`.** When absent, every profiling code path (PerfLayer, PerfOverlay mount, action breadcrumbs, cmd+shift+P shortcut, on-exit report) is dormant.
- **Filesystem watcher does not currently rebuild on `add_root` / `remove_root`.** New roots aren't watched until the next launch. Documented gap, low priority because the indexing pipeline that those commands trigger covers the immediate rescan.

## Coverage

This subsection enumerates what was inspected during this upkeep run, satisfying the knowledge-gap obligation.

### 2026-04-26 autonomous perf-bundle session (Tier 1 + Tier 2 + Phase 4-7)

Inspected in full this session (in addition to the prior 2026-04-26 evening pass):

- `src-tauri/src/db/mod.rs` — added `reader: OnceLock<Mutex<Connection>>` + `read_lock()` helper + `checkpoint_passive()` + new PRAGMAs in `initialize`.
- `src-tauri/src/db/embeddings.rs` — added `upsert_embeddings_batch` (R1); routed `get_all_embeddings`, `get_all_embeddings_for`, `count_embeddings_for` through the read-only secondary.
- `src-tauri/src/db/images_query.rs` — routed `get_images_with_thumbnails`, `get_images`, `get_image_id_by_path`, `get_pipeline_stats` through the read-only secondary; added `AggregatedRow`/`AggregatedValue` type aliases (clippy fix).
- `src-tauri/src/db/schema_migrations.rs` — bumped `CURRENT_PIPELINE_VERSION` 2 → 3.
- `src-tauri/src/indexing.rs` — switched both encoder loops (`run_clip_encoder` + `run_trait_encoder`) to `upsert_embeddings_batch` + `checkpoint_passive` between batches; legacy_clip_too=false (R8).
- `src-tauri/src/similarity_and_semantic_search/ort_session.rs` — NEW. Shared M2-tuned `Session` builder. Used by every encoder constructor.
- `src-tauri/src/similarity_and_semantic_search/encoder.rs`, `encoder_dinov2.rs`, `encoder_siglip2.rs`, `encoder_text/encoder.rs` — all re-routed through `build_tuned_session`. Text encoder gained real-input pre-warm (`encode("warmup")`) inside `new`.
- `src-tauri/src/similarity_and_semantic_search/cosine/rrf.rs` — NEW. RRF algorithm + 6 unit tests.
- `src-tauri/src/similarity_and_semantic_search/cosine/mod.rs` — added `pub mod rrf`.
- `src-tauri/src/lib.rs` — added `FusionIndexState`; extended `TextEncoderState` to two slots (CLIP + SigLIP-2); added crate-level `#![allow(clippy::doc_lazy_continuation)]`.
- `src-tauri/src/commands/similarity.rs` — added `get_fused_similar_images` IPC.
- `src-tauri/src/commands/semantic.rs` — full rewrite: `text_encoder_id` parameter dispatches CLIP vs SigLIP-2; helper `encode_with_clip` / `encode_with_siglip2`.
- `src-tauri/src/commands/roots.rs` — wired `fusion_state.invalidate_all()` next to existing `cosine_state.invalidate()` in 3 places.
- `src-tauri/src/perf.rs` — added `spawn_system_sampler_thread` (1Hz RSS/CPU via sysinfo).
- `src-tauri/src/perf_report.rs` — added `section_stall_analysis` + `section_resource_trends` + `percentile_summary` helper.
- `src-tauri/src/main.rs` — calls `spawn_system_sampler_thread` next to existing `spawn_flush_thread`; collapsed identical if-branches in env_filter (clippy).
- `src-tauri/src/thumbnail/generator.rs` — full rewrite of `generate_thumbnail`: `decode_jpeg_scaled` (R7) + `resize_with_fir` (R6) helpers added.
- `src-tauri/src/paths.rs` — `strip_windows_extended_prefix` switched to idiomatic `strip_prefix` (clippy).
- `src-tauri/Cargo.toml` — added `fast_image_resize 6`, `jpeg-decoder 0.3` (explicit), `sysinfo 0.32`.
- Frontend: `src/services/images.ts` (added `fetchFusedSimilarImages` + `textEncoderId` arg to `semanticSearch`), `src/queries/useSimilarImages.ts` (routed through fusion), `src/queries/useSemanticSearch.ts` (threads `prefs.textEncoder`), `src/components/settings/EncoderSection.tsx` (removed experimental warning), `src/services/services.test.ts` (3 new fusion tests + 1 textEncoderId test).
- Tests: `src-tauri/tests/cosine_topk_partial_sort_diagnostic.rs`, `src-tauri/tests/similarity_integration_test.rs` (clippy fixes).

### Inspected in full this run (2026-04-26 evening — earlier pass)

- **Encoder rewrites this session**: `encoder.rs` (CLIP image — full rewrite to separate vision_model.onnx + canonical preprocessing + L2-normalize), `encoder_text/encoder.rs` (CLIP text — full rewrite to HF tokenizers + OpenAI English text_model + max 77 + pad 49407), `encoder_dinov2.rs` (full rewrite to DINOv2-Base + canonical resize-256 + center-crop-224 + ImageNet stats + CLS slice), `encoder_siglip2.rs` (full rewrite to onnx-community URL + 256×256 stretch + Gemma SP + pooler_output + no attention_mask).
- **Diagnostic infrastructure**: `cosine/diagnostics.rs` (new — 4 stateless helpers), `cosine/index.rs` (added 4 diagnostic emissions), `lib.rs` (added cosine_math_sanity startup diagnostic), `commands/semantic.rs` (added tokenizer_output + query_embedding + score_distribution + path_resolution_outcomes diagnostics), `commands/similarity.rs` (added score_distribution + path_resolution + cross_encoder_comparison once-per-session).
- **Migration system**: `db/schema_migrations.rs` (added migrate_embedding_pipeline_version + meta table), `db/mod.rs` (wired the migration after embeddings table create).
- **Encoder picker**: `commands/encoders.rs` (re-added SigLIP-2; updated DINOv2 to Base/768-d).
- **Model download**: `model_download.rs` (new URLs/filenames for all 8 files; per-file fail-soft).
- **Indexing**: `indexing.rs` (encoder loops gained encoder_run_summary + preprocessing_sample emissions; switched DINOv2 references to use the new constant).

### Inspected in this upkeep pass (just now, for documentation)

- All system docs in `context/systems/` for staleness assessment.
- `notes.md`, all 8 notes files (after creating preprocessing-spatial-coverage.md, updating clip-preprocessing-decisions.md).
- `context/architecture.md`, `context/plans/perf-diagnostics.md`.
- `git status` + `git log --oneline -10` to confirm uncommitted vs committed scope.

### Noted but not read in full this run

- `src-tauri/src/db/{tags,thumbnails,roots,notes_orphans,test_helpers}.rs` — small files unchanged this session; behaviour assumed unchanged from prior upkeep notes.
- `src-tauri/src/{filesystem,paths,settings,perf_report}.rs` and the `commands/{images,tags,notes,profiling,roots}.rs` set — unchanged this session.
- Frontend changes from this session (`src/components/settings/EncoderSection.tsx` — read at session start to understand the picker; not re-inspected).
- `commands/profiling.rs`, `perf.rs` (line 100+), `perf_report.rs` past the diagnostic-rendering area.

### Inferred from structure / source comments only

- The exact serde shape of the `RawEvent::Diagnostic { ts_ms, diagnostic, payload }` variant in `perf.rs` — inferred from the section comment in conventions plus the diagnostic emission sites.
- Frontend behaviour around the encoder picker — inferred from EncoderSection.tsx read at session start; the actual TanStack Query / IPC path was not re-traced.
- `src/components/ui/*.tsx` (shadcn primitives) — derivative; not read.

### Verification questions deferred to next session

- Does the SigLIP-2 text encoder's ONNX I/O signature match what `Siglip2TextEncoder::encode` expects when actually called against a real model file? The prior research pass verified the signature externally; we haven't run a smoke test against the downloaded file.
- Does the embedding-pipeline migration trigger reliably on a fresh DB (no `meta` row at all)? Tests pass for the `meta` table create + insert path; the "stored is None" branch is exercised by the test suite indirectly via `fresh_db()`.
- Will the cross_encoder_comparison diagnostic's per-other-encoder temporary `CosineIndex` build complete in reasonable time on a 10k-image library? The estimated 50-200ms × encoders is from the prior research pass — not measured.
- Does the dim mismatch from prior `dinov2_small` rows existing in the embeddings table create any issue at startup before the migration runs? The migration runs inside `initialize()` so the cache should never see them — but worth confirming on a real upgrade scenario.
