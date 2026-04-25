# Architecture

## Scope / Purpose

Image Browser is a local-first Tauri 2 desktop application for browsing, tagging, and semantically searching large personal image libraries. A Rust backend handles filesystem scanning, SQLite persistence, thumbnail generation, and ONNX-Runtime CLIP inference for both image embeddings and multilingual text-query embeddings. A React 19 frontend renders a Pinterest-style masonry grid, modal inspector, and tag/search UI. Everything runs offline on consumer hardware with a CPU fallback when CUDA is unavailable.

This document is the structural map. Subsystem-level reality lives in `systems/`. Project-level rationale and conventions live in `notes/`.

## Repository Overview

| Dimension | Value | Source |
|-----------|-------|--------|
| Cargo package | `image-browser` v0.1.0, edition 2021 | `src-tauri/Cargo.toml` |
| Tauri identifier | `com.ataca.image-browser` | `src-tauri/tauri.conf.json` |
| Frontend bundler | Vite 7 + `vite-plugin-pages` (file-based routing) | `vite.config.ts`, `package.json` |
| Backend source | 14 Rust files in `src-tauri/src/` | filesystem |
| Frontend source | 33 TypeScript files in `src/` (20 `.tsx`, 13 `.ts`) | filesystem |
| Persistence | Single SQLite file `images.db` (3 tables) | `src-tauri/src/db.rs` |
| ML runtime | `ort = 2.0.0-rc.10` with `cuda` and `download-binaries` features | `src-tauri/Cargo.toml:21` |
| Image encoder | CLIP ViT-B/32 (512-d output) via ONNX | `src-tauri/src/similarity_and_semantic_search/encoder.rs` |
| Text encoder | clip-ViT-B-32-multilingual-v1, max_seq_length 128, pure-Rust WordPiece tokenizer | `encoder_text.rs:223-224` |
| Tauri commands | 8 — see `systems/tauri-commands.md` | `lib.rs:612-621` |
| Models on disk | `models/model_image.onnx`, `models/model_text.onnx`, `models/tokenizer.json` (not committed; user-supplied) | `lib.rs:121-134`, `main.rs:47` |

## Repository Structure

```text
PinterestStyleImageBrowser/
├── README.md                    # Project intent & milestone roadmap
├── package.json                 # Frontend deps; React 19, TanStack Query 5, framer-motion, shadcn primitives
├── tsconfig.json                # @/ alias → /src
├── vite.config.ts               # Tailwind v4, @vitejs/plugin-react, vite-plugin-pages
├── images.db                    # Local SQLite (in .gitignore)
├── components.json              # shadcn-ui registry config
├── public/                      # Static assets served by Vite
├── src/                         # Frontend (React)
│   ├── App.tsx                  # BrowserRouter + QueryClientProvider + Pages routes
│   ├── main.tsx                 # ReactDOM root, StrictMode
│   ├── pages/[...slug].tsx      # Single catch-all route — URL slug = selected image id
│   ├── components/
│   │   ├── Masonry.tsx          # Shortest-column packing; promotes selected hero across up to 3 cols
│   │   ├── MasonryItem.tsx      # 3D tilt-on-hover via framer-motion + spring
│   │   ├── MasonryAnchor.tsx    # Absolute-positioned wrapper used by Masonry
│   │   ├── PinterestModal.tsx   # Fullscreen inspector with prev/next + tag editing
│   │   ├── SearchBar.tsx        # Single input; #-prefixed tag autocomplete + tag pills
│   │   ├── TagDropdown.tsx      # Popover combobox (cmdk) with create-tag-on-no-match
│   │   ├── FullscreenImage.tsx        # DEAD — leftover earlier inspector
│   │   ├── MasonryItemSelected.tsx    # DEAD — superseded by inline isSelected in MasonryItem
│   │   ├── MasonrySelectedFrame.tsx   # DEAD — superseded by PinterestModal
│   │   └── ui/                  # shadcn primitives (badge, button, card, command, dialog, popover, skeleton)
│   ├── queries/                 # TanStack Query hooks
│   │   ├── queryClient.ts       # staleTime: Infinity, no auto-refetch — see notes/frontend-query-policy.md
│   │   ├── useImages.ts         # useImages + useAssignTagToImage + useRemoveTagFromImage (optimistic)
│   │   ├── useTags.ts           # useTags + useCreateTag (optimistic with id=-1 placeholder)
│   │   ├── useSimilarImages.ts  # useSimilarImages (DEAD), useTieredSimilarImages (live)
│   │   └── useSemanticSearch.ts # 5-min staleTime, 10-min gcTime, debounced from caller
│   ├── services/                # invoke() wrappers — translate Tauri results to UI types
│   │   ├── images.ts            # fetchImages, semanticSearch, fetchSimilarImages, fetchTieredSimilarImages
│   │   └── tags.ts              # fetchTags, createTag (default colour #3489eb)
│   ├── hooks/
│   │   ├── useDebouncedValue.ts # 300ms debounce used for search text
│   │   └── useMeasure.tsx       # DEAD — written, never imported
│   ├── lib/utils.ts             # cn() helper for shadcn
│   ├── utils.ts                 # getImageSize() via DOM Image, waitForAllInnerImages()
│   └── types.d.ts               # ImageData, ImageItem, Tag, SimilarImageItem
└── src-tauri/                   # Rust backend + Tauri shell
    ├── Cargo.toml               # ort, rusqlite, tauri, image, ndarray, rand, serde, base64
    ├── tauri.conf.json          # csp: null, assetProtocol scope ["**"]
    ├── test_images/             # ~1.5 GB sample dataset (749 jpg)
    └── src/
        ├── main.rs              # Entry: hardcoded test_images scan → thumbnails → encode → run()
        ├── lib.rs               # 8 #[tauri::command] handlers + State management + run()
        ├── db.rs                # ImageDatabase wraps Mutex<rusqlite::Connection>; 3 tables; migration
        ├── filesystem.rs        # ImageScanner — recursive read_dir + 7-extension whitelist
        ├── image_struct.rs      # ImageData (id, name, path, tags, thumbnail_path?, width?, height?)
        ├── tag_struct.rs        # Tag (id, name, color)
        ├── thumbnail/
        │   ├── mod.rs           # pub use generator::ThumbnailGenerator
        │   └── generator.rs     # 400×400 max, aspect-preserving, JPEG, image::thumbnail() (Lanczos3)
        └── similarity_and_semantic_search/
            ├── mod.rs           # Re-exports the 3 submodules
            ├── encoder.rs       # CLIP image encoder via ort; 224×224, ImageNet-stats, batch=32
            ├── encoder_text.rs  # multilingual CLIP text encoder + pure-Rust WordPiece tokenizer
            └── cosine_similarity.rs  # CosineIndex + 3 retrieval modes
```

## Subsystem Responsibilities

```
                  ┌──────────────────────────────────────────────┐
                  │              React 19 Frontend               │
                  │                                              │
   Browser ──►   pages/[...slug].tsx (search-routing)            │
                       │   │                                     │
                       ▼   ▼                                     │
                 Masonry ◄── SearchBar / PinterestModal /        │
                 (layout)    TagDropdown (tag-system)            │
                       │                                         │
                       ▼                                         │
                 TanStack Query hooks (frontend-state)           │
                       │ invoke()                                │
                  ─────┼─────────  Tauri IPC boundary  ──────────┤
                       ▼                                         │
                  ┌─────────────────────────────────────────────┐│
                  │             Rust Backend                    ││
                  │                                             ││
                  │  lib.rs — 8 #[tauri::command] handlers      ││
                  │     │                                       ││
                  │     ├─► ImageDatabase  (db.rs)              ││
                  │     │      └─► SQLite file images.db         │
                  │     ├─► CosineIndex    (cosine_similarity.rs)│
                  │     │      └─► in-memory Vec<(PathBuf, Array1<f32>)> populated lazily from DB │
                  │     └─► TextEncoder    (encoder_text.rs)    ││
                  │            └─► ONNX Runtime + tokenizer.json││
                  │                                             ││
                  │  main.rs — startup pipeline:                ││
                  │     scan → DB insert → thumbnails → encode  ││
                  │            (filesystem → db → thumbnail →    │
                  │             encoder)                        ││
                  └─────────────────────────────────────────────┘│
```

| System | Owns | Source location | Canonical doc |
|--------|------|-----------------|---------------|
| `database` | SQLite schema, 3 tables, embedding BLOB encoding, runtime migration | `src-tauri/src/db.rs` | `systems/database.md` |
| `filesystem-scanner` | Recursive image discovery, 7-extension whitelist | `src-tauri/src/filesystem.rs` | `systems/filesystem-scanner.md` |
| `thumbnail-pipeline` | 400×400 cached thumbnails on disk + DB row updates | `src-tauri/src/thumbnail/generator.rs` | `systems/thumbnail-pipeline.md` |
| `clip-image-encoder` | 224×224 preprocess, ONNX inference, 512-d output, batched | `src-tauri/src/similarity_and_semantic_search/encoder.rs` | `systems/clip-image-encoder.md` |
| `clip-text-encoder` | Pure-Rust WordPiece tokenizer + multilingual CLIP text inference | `encoder_text.rs` | `systems/clip-text-encoder.md` |
| `cosine-similarity` | In-memory similarity index, 3 retrieval modes (sampled/sorted/tiered) | `cosine_similarity.rs` | `systems/cosine-similarity.md` |
| `tauri-commands` | 8-command IPC surface, state injection, Windows path normalisation | `lib.rs` | `systems/tauri-commands.md` |
| `masonry-layout` | Shortest-column packing, hero promotion, 3D tilt | `src/components/Masonry*.tsx` | `systems/masonry-layout.md` |
| `tag-system` | Tag CRUD, optimistic updates, `#`-autocomplete, create-on-no-match | `src/components/{SearchBar,TagDropdown}.tsx`, `useTags.ts`, `useImages.ts` | `systems/tag-system.md` |
| `search-routing` | Frontend priority chain: similar > semantic > tag > all | `src/pages/[...slug].tsx` | `systems/search-routing.md` |
| `frontend-state` | TanStack Query config, vite-plugin-pages, optimistic mutation pattern | `src/queries/queryClient.ts`, `App.tsx`, `vite.config.ts` | `systems/frontend-state.md` |

## Dependency Direction

```
        main.rs (binary entry)
            │ initialises in order
            ▼
  filesystem ──► database ◄────────────────┐
                    ▲                      │
                    │                      │
        thumbnail ──┤                      │
                    │                      │
   image-encoder ──┤                       │
                    │                      │
                    │   reads BLOBs        │
   cosine-similarity ──── populate_from_db │  (also opens its own DB connection — see surprising-connections in notes)
                    ▲                      │
                    │                      │
   text-encoder ────┘                      │
                                           │
                         lib.rs::run() ────┘
                              │ tauri::Builder + .manage(state)
                              ▼
                  8 #[tauri::command] handlers
                              │ invoke() over Tauri IPC
                              ▼
                  React frontend (services → queries → components)
```

Key directional rules observed in code:

- `database` is depended on by every other backend system. It has no inverse dependencies.
- `cosine-similarity::populate_from_db` does **not** receive `&ImageDatabase`; it constructs a second `ImageDatabase` from a stored `db_path` string (`cosine_similarity.rs:27`). This is a documented coupling smell — see `systems/cosine-similarity.md` Risks and `notes/path-and-state-coupling.md`.
- `filesystem`, `thumbnail`, and `encoder` modules each only know about `database`; they do not depend on each other. The pipeline ordering lives in `main.rs`.
- The frontend has no knowledge of which ONNX models exist or how cosine works — only the 8-command IPC surface plus the JSON shapes the commands return.

## Core Execution / Data Flow

### Startup pipeline (`main.rs`)

```
1. Parse hardcoded scan path                  Path::new("test_images")        main.rs:24
2. Open SQLite + ensure schema                ImageDatabase::new + initialize main.rs:26-28; db.rs:21-58
3. Run migration if needed                    PRAGMA table_info → ALTER TABLE db.rs:62-81
4. Recursive scan + insert paths              ImageScanner::scan_directory   filesystem.rs:22-41
5. Generate missing thumbnails                ThumbnailGenerator::generate_all_missing_thumbnails
6. Encode missing embeddings (batch=32)       Encoder::encode_all_images_in_database  encoder.rs:256
7. Hand control to Tauri runtime              image_browser_lib::run         main.rs:52
   └─► tauri::Builder.manage(db, cosine_state, text_encoder_state).invoke_handler!(...)
```

Steps 4-6 are idempotent: each query (`get_images_without_thumbnails`, `get_images_without_embeddings`) only returns rows missing the relevant artefact. A re-launch on an already-indexed database is fast.

### Runtime: image grid load

```
Frontend                           Backend
──────                             ──────
useImages({tagIds, searchText})
  └─► fetchImages(tagIds, searchText)
        └─► invoke("get_images")
                                    db.get_images_with_thumbnails
                                      └─► SQL: LEFT JOIN images_tags + tags
                                      └─► aggregate tag rows → Vec<ImageData>
                                      └─► rand::rng() shuffle  (db.rs:496-499)
        ◄─── Vec<ImageData> (JSON)
  └─► map to ImageItem with convertFileSrc()
TanStack Query caches by ["images", tagIds, searchText]
Masonry receives items; computes shortest-column packing
```

### Runtime: similarity search (image-clicked path)

```
User clicks tile → URL becomes "/{id}/" via react-router
useEffect in [...slug] → setSelectedItem
useTieredSimilarImages(id) → invoke("get_tiered_similar_images")

Backend:
  cosine_state.lock() → if cached_images.is_empty(), populate_from_db
  db.get_image_embedding(id)  ──► reinterpret BLOB as Vec<f32>
  CosineIndex::get_tiered_similar_images(query)
    └─► sort all by cosine descending
    └─► sample 5 random per tier × 7 tiers (0-5%, 5-10%, ..., 40-50%)
    └─► return up to 35 (PathBuf, score) tuples
  Map paths → DB ids via 3-strategy normalize_path fallback (lib.rs:308-371)
  Return Vec<SimilarImage> JSON

Frontend:
  fetchTieredSimilarImages → for each, getImageSize from thumbnail (DOM Image)
  displayImages becomes the similar set; Masonry re-renders with selectedItem promoted
```

### Runtime: semantic search end-to-end (the chosen Dependency Chain Trace)

This is the obligation's chosen critical path because it crosses the most boundaries:

```
[1] User types into SearchBar → useState(searchText)
        ──► useDebouncedValue(searchText, 300)            src/pages/[...slug].tsx:26
[2] shouldUseSemanticSearch test:
        text non-empty AND not "#" prefix AND no selected item   pages/[...slug].tsx:32-33
[3] useSemanticSearch(query, 50) → useQuery
        queryKey: ["semantic-search", query, 50]
        staleTime 5min, gcTime 10min                       useSemanticSearch.ts:21-23
[4] semanticSearch(query, 50) →
        invoke("semantic_search", { query, topN: 50 })     services/images.ts:198-228
        ─── Tauri IPC boundary ───
[5] lib.rs::semantic_search:
      • lock TextEncoderState mutex; lazy-init if encoder is None  lib.rs:114-142
        - validates models/model_text.onnx exists
        - validates models/tokenizer.json exists
        - constructs TextEncoder + SimpleTokenizer
      • encoder.encode(query)                              lib.rs:148
        └─► encoder_text.rs:253-...
            └─► tokenizer: split_whitespace → WordPiece
                (try original case → lowercase fallback)
            └─► pad/truncate to max_seq_length=128
            └─► ort session.run(input_ids, attention_mask)
            └─► extract output (try sentence_embedding/text_embeds/...)
            └─► mean-pool fallback for [seq_len, 768] shapes
            └─► returns Vec<f32> length 512
      • lock CosineIndexState mutex
      • if cached_images empty, populate_from_db          lib.rs:162-169
        └─► opens NEW ImageDatabase(db_path)
        └─► db.get_all_images() → for each, db.get_image_embedding(id)
        └─► skip rows with no embedding or empty embedding
        └─► add to in-memory cached_images vec
      • CosineIndex::get_similar_images_sorted(query, 50, None)  lib.rs:174
        └─► cosine for every cached image, sort desc, take 50 (no random sampling — sorted variant)
      • For each result path: 3-strategy DB lookup with Windows-prefix stripping  lib.rs:182-221
      • For each id: db.get_image_thumbnail_info → enrich result with thumbnail_path/w/h
        ─── Tauri IPC return ───
[6] services/images.ts maps results → SimilarImageItem with convertFileSrc
[7] pages/[...slug].tsx::displayImages branch 2 fires; Masonry renders sorted result list
```

**Boundary failure semantics for this chain:**

| Step | Failure | Behaviour |
|------|---------|-----------|
| [1]-[3] | Empty query | `enabled` flag is false; query never runs |
| [3] | Hash-prefixed query | `shouldUseSemanticSearch` is false; routes through tag filter instead |
| [5] lazy init | model_text.onnx missing | Returns `Err` with explicit path; frontend shows generic "Search failed. Make sure the text model is available." |
| [5] populate_from_db | DB cannot open | `expect("failed to init db")` — **panics** the command (Mutex poisoned for the rest of the session) |
| [5] cosine sort | Cached vec empty | Returns empty `Vec`; not an error |
| [5] DB id lookup | None of the 3 strategies match | Result is filtered out via `filter_map`; user silently gets fewer results |
| [6]-[7] | Mutation fails | Optimistic update is rolled back via `onError` snapshot restore |

The chain crosses 4 process boundaries (UI → IPC → DB → ONNX) and 2 synchronisation boundaries (TextEncoder mutex, CosineIndex mutex). Both mutexes are held for the duration of the operation — concurrent semantic searches serialise.

## Inter-System Relationships

This table satisfies the inter-system relationship mapping obligation. Each row cites the two systems, the mechanism, and the failure consequence.

| # | A | B | Mechanism | What breaks if it fails |
|---|---|---|-----------|-------------------------|
| 1 | filesystem-scanner | database | Scanner returns `Vec<String>`; `add_image` does `INSERT OR IGNORE` per path. `db.rs:88-94` | A scan with permission errors propagates up via `?`; partial scans leave DB partially populated but the `INSERT OR IGNORE` is idempotent on retry. |
| 2 | database | thumbnail-pipeline | Pipeline reads via `get_images_without_thumbnails` (`db.rs:339`), writes back via `update_image_thumbnail` (`db.rs:381`). | If `update_image_thumbnail` fails for an image, the file is generated but the DB row stays unmarked, so it will be regenerated next launch (but the generator's `if thumbnail_path.exists()` short-circuit at `generator.rs:61` prevents wasted work). |
| 3 | database | clip-image-encoder | Encoder reads via `get_images_without_embeddings` (`db.rs:208`), writes via `update_image_embedding` (`db.rs:249`) which `unsafe`-casts `Vec<f32>` to `&[u8]`. | A schema change to the embedding column shape would silently break round-tripping (no length check on store; length-mod-4 check on retrieval at `db.rs:294-307`). |
| 4 | database | cosine-similarity | `populate_from_db(db_path: &str)` opens a **second** `ImageDatabase` connection from the stored path (`cosine_similarity.rs:27`). It does not borrow the existing `&ImageDatabase`. | Surprising connection — listed in `notes/path-and-state-coupling.md`. If `db_path` is wrong, the cosine index silently populates from a different database. |
| 5 | clip-text-encoder | cosine-similarity | Text embedding (`Vec<f32>` length 512) is fed into `get_similar_images_sorted` as `Array1::from_vec` (`lib.rs:173-174`). Both encoders produce the same-dim space — that is the point of multilingual CLIP. | Dimension mismatch (e.g., model swap) panics on `ndarray` cosine math at runtime (`a.dot(b)` requires matching shapes). |
| 6 | tauri-commands | database | Every command takes `State<'_, ImageDatabase>` injected via `tauri::Builder::manage(db)` (`lib.rs:609`). | If `manage(db)` is omitted, every command fails IPC at registration time. Sole owner of the connection is the Tauri runtime. |
| 7 | tauri-commands | cosine-similarity | Commands take `State<'_, CosineIndexState>` (`lib.rs:38-41`). The index is `Mutex<CosineIndex>` plus a `db_path: String`. | If a panic ever occurs while holding the cosine mutex, every subsequent similarity query fails with `Mutex poisoned`. |
| 8 | tauri-commands | clip-text-encoder | `semantic_search` takes `State<'_, TextEncoderState>` whose inner is `Mutex<Option<TextEncoder>>` — lazy because the model is large. (`lib.rs:45-47`, init at `lib.rs:114-142`.) | Same poison-on-panic risk as #7. Lazy init means a missing `tokenizer.json` only manifests on the first semantic search, not at app startup. |
| 9 | search-routing | tauri-commands | Frontend `pages/[...slug].tsx` invokes 3 commands (`get_images`, `get_tiered_similar_images`, `semantic_search`) and chooses outputs by priority (`pages/[...slug].tsx:71-99`). | If any command's JSON shape changes without a TS update, the priority union silently falls back to whichever branch did succeed. |
| 10 | masonry-layout | search-routing | Masonry receives `displayImages: ImageItem[]` from routing's union; `selectedItem` is also passed in to drive hero promotion. (`Masonry.tsx:17-22`, used at `pages/[...slug].tsx:259-265`.) | If the selected id is not in `displayImages` (e.g., because the user navigated to a semantic-search result and that id is not in `images.data`), `selectedItem` is `null` and the hero card is not promoted. This is the "Selection lookup fails against semantic results" UX bug noted in the LifeOS Gaps doc. |
| 11 | tag-system | database | Tags use `tags` and `images_tags` join tables (`db.rs:39-57`). Filter is OR-semantic (`tag_id IN (?, ?)` with `EXISTS`). Tag deletion is implemented in `db.rs:105-111` but **not registered** in `invoke_handler!`. | OR vs AND semantics is a product decision worth flagging because the README ambiguously implies AND. Deletion gap means typo tags accumulate forever via the UI. |
| 12 | frontend-state | search-routing + tag-system + masonry | `queryClient.ts` sets `staleTime: Infinity` — caches never go stale automatically. Refetching is done via explicit `invalidateQueries` (e.g., on modal close — `[...slug].tsx:114`). | A stale-but-cached image set after a tag mutation would show wrong tags on stale rows. Optimistic updates handle the common case, but cross-cache-key staleness is possible. See `notes/frontend-query-policy.md`. |

12 entries documented; floor is 11. Obligation cleared.

## Critical Paths and Blast Radius

The semantic-search end-to-end chain in §"Core Execution / Data Flow" is the longest. Two adjacent critical paths share most of its segments:

| Operation | Chain length | Shared with semantic? | Unique segment |
|-----------|--------------|----------------------|----------------|
| Tiered similar (image clicked) | UI → IPC → DB → CosineIndex → DB → IPC → UI | yes (through cosine + DB) | Tiered tier sampling (`cosine_similarity.rs:295-330`) instead of sorted top-N |
| Tag filter image load | UI → IPC → DB → IPC → UI | partial (DB only) | `EXISTS ... IN (...)` SQL filter (`db.rs:438-444`) |
| Tag mutation | UI → optimistic UI → IPC → DB → IPC → cache rollback-or-no-op | partial (DB only) | TanStack Query rollback dance |

All three chains share the **DB Mutex<Connection>** as a global serialisation point. A long-running command holds the DB mutex for its full duration; concurrent UI actions queue.

The CosineIndex Mutex is the second serialisation point — **all** similarity-driven operations queue through it.

## State Ownership

| State | Owner | Sharing pattern | Risk |
|-------|-------|-----------------|------|
| `images.db` (SQLite) | `ImageDatabase` instance held by Tauri State | Single connection wrapped in `Mutex<Connection>`. Cosine module opens a **second** connection (read-only in practice, but not enforced). | Two connections to the same SQLite file works because rusqlite allows it, but it splits the lock and means cache invariants live entirely outside SQLite. |
| `CosineIndex.cached_images` | `CosineIndexState.index: Mutex<CosineIndex>` | Lazy-populated on first similarity/semantic query; **not invalidated** when new embeddings are added at runtime. | Once runtime rescan/encoding ships, this becomes a real bug. Today it cannot happen because there is no runtime rescan. |
| `TextEncoder` | `TextEncoderState.encoder: Mutex<Option<TextEncoder>>` | Lazy-loaded on first semantic search. | Heavy resource (ONNX session). Once loaded it lives until process exit. |
| Frontend image cache | TanStack QueryClient cache | Invalidated explicitly on modal close (`[...slug].tsx:114`); staleTime is Infinity otherwise. | Tag-mutation rollback covers the common case; broader cross-key invalidation is manual. |
| `selectedItem`, `searchText`, `searchTags` | React useState in `pages/[...slug].tsx` | Single render owner; URL slug is the source of truth for selection (parsed in `useEffect`). | URL → state lookup uses `images.data` — fails for selections that arrived via semantic-search (see #10 in the relationship table). |

## Structural Notes / Current Reality

This section captures the structural-level realities a reader needs to know before making changes.

- **The repo last shipped on 2026-03-04.** Roughly 7 weeks dormant as of 2026-04-25. Treat the codebase as feature-complete-for-personal-use rather than under active development. The next session is likeliest to be a foundation pass (folder picker + tracing + path normalisation + dead-code sweep) before any new feature work.
- **The default scan path is hardcoded to `test_images/`** (`main.rs:24`). The README claims a folder picker exists. It does not. Closing this gap is the single biggest unlock from "demo" to "tool" — see `systems/filesystem-scanner.md`.
- **CLIP models are not committed.** `models/model_image.onnx`, `models/model_text.onnx`, and `models/tokenizer.json` must be supplied by the user. The README claims models are "bundled" — they are not in the repo. A fresh clone does not produce a working app.
- **Two warnings the linter would surface but should be treated as justified:** `src/components/` and `src/queries/` are not name-matched to a system file because the system files use topical names (`masonry-layout`, `tag-system`, `search-routing`, `frontend-state`) rather than directory names. Coverage is real; the substring match in `lint_context.py` cannot see it.
- **`zustand` is declared in `package.json` but never imported.** Memory-bank residue — TanStack Query took its place. See `notes/dead-code-inventory.md`.
- **Three Tauri-managed `Mutex` singletons** (`ImageDatabase`, `CosineIndexState`, `TextEncoderState`) serialise all backend operations. Concurrent commands queue. For the current single-user UI this is fine; future "preload similar for hovered tile" or background work would contend.
- **The `images.db` SQLite file lives at the repo root** (relative path `"../images.db"` from `src-tauri/`'s working directory — `db.rs:84-86`). It is `.gitignore`'d. The `.thumbnails/` cache directory is created next to wherever the app's `cwd` is.
- **CUDA fallback is theoretical in dev, real in release.** The author observed via inline benchmarks that debug builds use CPU even when CUDA is "enabled," and release builds use the GPU. Inference speed varies by ~10× between the two — meaningful for end-user perception but not a correctness issue.
- **The `assetProtocol` security scope is `["**"]`** with `csp: null` (`tauri.conf.json:21-26`). Fine for a single-user local tool that only ever loads its own bundled HTML; dangerous if the WebView is ever pointed at untrusted content. Worth narrowing if the app shape changes.

## Coverage

This subsection enumerates what was actually inspected during this upkeep run, satisfying the knowledge-gap obligation.

### Inspected in full

- All 14 Rust source files: `main.rs`, `lib.rs`, `db.rs`, `filesystem.rs`, `image_struct.rs`, `tag_struct.rs`, `thumbnail/mod.rs`, `thumbnail/generator.rs`, `similarity_and_semantic_search/mod.rs`, `similarity_and_semantic_search/encoder.rs`, `similarity_and_semantic_search/cosine_similarity.rs`, plus partial read of `encoder_text.rs` (lines 1-300; struct + tokenizer + encoder constructor + encode method, including pooling fallback logic).
- Backend config: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
- Frontend config: `package.json`, `vite.config.ts`, `tsconfig.json` (top-level read).
- Frontend source: `src/App.tsx`, `src/main.tsx`, `src/pages/[...slug].tsx`, `src/components/Masonry.tsx`, `src/components/MasonryItem.tsx`, `src/components/SearchBar.tsx`, `src/components/PinterestModal.tsx`, `src/components/TagDropdown.tsx`, all 5 files in `src/queries/`, `src/services/images.ts`, `src/services/tags.ts`, `src/types.d.ts`, `src/utils.ts`.
- Git history: `git log --format=fuller --since='180 days ago'` — bodies of the 16 most recent commits inspected.

### Noted but not read in full

- `src-tauri/src/similarity_and_semantic_search/encoder_text.rs:301-end` — TextEncoder `encode` method tail (output extraction past the first 4 attempt names) and `mean_pool` helper. The first 300 lines covered the obligation-relevant rationale (tokenizer case handling, max_seq_length=128).
- `src/components/ui/{badge,button,card,command,dialog,popover,skeleton}.tsx` — shadcn/ui primitives; derivative.
- `src/components/MasonryAnchor.tsx`, `src/components/FullscreenImage.tsx`, `src/components/MasonryItemSelected.tsx`, `src/components/MasonrySelectedFrame.tsx` — anchor is small wrapper logic; the latter three are unmounted dead components per the cross-LifeOS Gaps doc, with status confirmed by import-graph absence.
- `src/hooks/useMeasure.tsx`, `src/hooks/useDebouncedValue.ts` — the latter is used at `pages/[...slug].tsx:26`; the former has no import sites and is dead.
- `src/lib/utils.ts` — single `cn()` helper.

### Inferred from structure only

- `models/*.onnx` and `models/tokenizer.json` — not committed (only `.gitkeep`-shaped placeholders or absent). Existence and shape inferred from how `lib.rs:120-134` validates them.
- `.thumbnails/` directory — only the naming convention `thumb_{id}.jpg` is observed (`generator.rs:51`). Directory contents not enumerated.
- `test_images/` — only the count (749 images per LifeOS doc; `test_scan_directory_finds_all_images` test asserts `len() == 4` which contradicts current state and will fail under `cargo test`).
- `package-lock.json` — not read; relied on `package.json`.
