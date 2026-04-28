# Image Browser

> A local-first desktop application for browsing and searching large image collections — Pinterest-style masonry layout, manual tagging, multi-encoder visual similarity search, and natural-language semantic search, all running entirely on your machine with no cloud, no accounts, and no external services.

---

## Why Image Browser exists

File explorers are built for files, not images. When you have thousands of images across nested folders — reference collections, inspiration boards, photography libraries, art assets — navigating them with a file explorer means clicking through directories one by one with no way to search by meaning, find visually similar images, or organise by anything other than filename or date.

Image Browser solves this by treating your local image library as a first-class collection: thumbnailed, indexed, tagged, annotated, and searchable — both by text labels and by semantic meaning. Type "dark cinematic lighting" or "forest path at dusk" and find matching images without having manually tagged them. Click any image to instantly surface the visually similar ones from across your entire library. Add per-image notes to capture context the filename can't.

Everything runs locally. Embeddings are generated on your machine using ONNX Runtime. The only network call the app ever makes is the first-launch download of the encoder models from HuggingFace; after that, you can disconnect the network entirely.

---

## Features

### Browsing

- **Pinterest-style masonry grid** with shortest-column packing and aspect-ratio-preserving thumbnails — handles tens of thousands of images without performance degradation
- **Infinite scroll** with virtualised loading via TanStack Query
- **Adjustable column count, animation level, and sort mode** (recent, random, stable)
- **Hover micro-interactions** and 3D tilt animations (toggleable for low-power preference)
- **Fullscreen modal inspector** with prev/next navigation, keyboard shortcuts, and inline tag/note editing
- **Slideshow mode** — fullscreen auto-advancing slideshow over any view: main feed, tag-filtered results, or search results

### Multi-folder library

- **Add multiple root folders** to a single library — each scanned recursively, with per-folder enable/disable toggles
- **Filesystem watcher** with 5-second debounce automatically picks up new, moved, or deleted files
- **Orphan detection** marks images whose source files have disappeared without losing their tags or notes
- **Per-root thumbnail isolation** so adding or removing a folder never invalidates other folders' caches

### Tagging

- **Manual tags** with optional colours, added and removed per image from the inspector or search bar
- **Tag autocomplete** with `#tag` syntax in the search bar; tags can be created on the fly by typing a new name
- **AND / OR tag filtering** — show images that match all selected tags or any of them
- **Tag deletion** from the search bar dropdown, with optimistic UI updates throughout

### Notes

- **Per-image notes** — free-form text captured in the inspector and persisted to the local database

### Visual similarity search (image → image)

- **Click any image** in the inspector to retrieve the most visually similar images from the entire library
- **Multi-encoder fusion** combining three independently-trained vision models via **Reciprocal Rank Fusion** (Cormack 2009, k=60):
  - **CLIP ViT-B/32** (OpenAI, 512-d) — strong on captionable visual concepts
  - **DINOv2-Base** (Meta, 768-d) — strong on self-supervised visual structure and texture
  - **SigLIP-2 Base 256** (Google, 768-d) — strong on full-image semantics with no centre-crop
- **Per-encoder toggles** in settings — enable any subset; the fusion ranker adapts automatically
- **Cosine-based ranking** with persistent on-disk cache that survives restarts

### Semantic search (text → image)

- **Natural-language queries** — type "skull", "neon cityscape", "dynamic pose", or any free-form phrase
- **Text-image fusion** runs the query through every enabled text-capable encoder (CLIP and SigLIP-2) and fuses the rankings via RRF, just like image-image search
- **Tag search and semantic search coexist** — exact tag matches take priority; otherwise the query is treated as semantic
- **Debounced live search** with 300 ms input debouncing and 5-minute result caching

### Settings drawer

- **Theme** — light, dark, or system
- **Display** — column count, animation level, image scale
- **Search** — result counts, tag-filter mode (AND/OR)
- **Sort** — recent, random, stable
- **Folders** — add, remove, enable/disable scan roots; trigger manual rescans
- **Encoders** — per-encoder enable/disable toggles for image and text directions
- **Reset** — clear preferences without touching the library

### Performance and observability

- **Parallel encoder execution** during indexing — all enabled encoders run concurrently with intra-thread tuning shared across the M2 P-cluster
- **JPEG fast path** using native scaled IDCT (`jpeg-decoder` 1/8, 1/4, 1/2 factor) followed by NEON-optimised Lanczos3 (`fast_image_resize`)
- **WAL-mode SQLite** with separate writer and read-only secondary connections, batched embedding upserts, and manual checkpointing between encoder batches
- **Live indexing-status pill** in the top-right showing scan, thumbnail, and encoder progress in real time
- **Optional profiling mode** (`--profiling` flag) — span timing, 1 Hz RSS/CPU sampler, named domain diagnostics, on-exit markdown report; zero overhead when off

### Privacy and offline operation

- **Local-only storage** — SQLite database, thumbnail cache, and ONNX model files all live in your platform's app-data directory (`~/Library/Application Support/com.ataca.image-browser/` on macOS)
- **No accounts, no telemetry, no API keys**
- **First-launch model download** is the only network call the app makes — once models are cached locally, the app runs fully offline
- **Original images are never modified or moved** — only metadata, thumbnails, and embeddings are derived

---

## How to use

### First launch

1. Launch the app. On first launch it will download the three encoder model bundles from HuggingFace (~2.5 GB total). Subsequent launches read these from disk.
2. Open the **Settings drawer** (gear icon, top-right) and add at least one folder under the **Folders** section. The folder is scanned recursively for images (`.jpg`, `.jpeg`, `.png`, `.webp`, `.gif`, `.bmp`, `.tiff`).
3. The **indexing-status pill** in the top-right tracks progress through three phases:
   - **Scan** — discovering image files on disk
   - **Thumbnails** — generating 400×400 cached previews
   - **Encoders** — generating embeddings via the enabled encoders
4. As thumbnails finish, images appear in the grid. As embeddings finish, similarity and semantic search become available for those images. Both run in the background; the grid is browsable immediately.

### Browsing the grid

- **Scroll** to load more images. The grid is infinite-scrolling with virtualised paging.
- **Click** any image to open the inspector modal.
- **Arrow keys** in the inspector navigate to the previous / next image.
- **Esc** closes the inspector.
- Use the **slideshow icon** to start a fullscreen auto-advancing slideshow over the current view — works on the main feed, tag-filtered results, and search results.

### Searching

- The search bar at the top of the page accepts both **tag queries** and **natural-language queries**.
- Type `#` to autocomplete from existing tags. Multiple tag pills can be combined; the AND/OR mode is configurable in the Search section of settings.
- Type plain text (no `#`) to run a **semantic search** across the library — for example, `forest path at dusk`, `geometric pattern`, or `portrait of a woman in red`.
- If your query matches an existing tag exactly, the tag filter takes priority; otherwise it is treated as semantic.

### Visual similarity

- Open any image in the inspector and click **View Similar** to retrieve images visually similar to it from across the library.
- Results are ranked by fused similarity across all enabled image encoders.
- The similarity view itself supports the slideshow icon — start a slideshow over the similarity ranking to flip through visually-related images at speed.

### Tagging and notes

- Open an image in the inspector. Use the **tag combobox** to add or remove tags; type a new name to create a tag on the fly.
- Use the **notes textarea** to capture any free-form context for the image. Notes are saved automatically.
- Tags are deletable from the search bar's autocomplete dropdown — useful for cleaning up stray tags.

### Encoder toggles

- Open settings and find the **Encoders** section. Each encoder (CLIP image, CLIP text, DINOv2 image, SigLIP-2 image, SigLIP-2 text) can be independently enabled or disabled.
- Disabling an encoder skips its computation during indexing and removes it from the fusion ranker. Re-enabling it triggers a background re-index for any images that don't yet have embeddings from that encoder.
- The fusion ranker operates over whichever encoders are enabled at query time.

### Profiling mode

If you're investigating performance, launch with the profiling flag:

```bash
npm run tauri dev -- -- --profiling
```

(The double `--` is required: the first separates `tauri` from its CLI, the second passes the flag through to the Rust binary.) An on-exit markdown report is written to the app-data directory. The flag is `--profiling`, not `--profile` — the latter collides with Tauri's own cargo-profile flag.

---

## Architecture

Image Browser runs as a Tauri 2 desktop application — React 19 frontend, Rust backend, SQLite persistence, ONNX Runtime inference.

```
┌─────────────────────────────────────────────────────────────┐
│                      React 19 Frontend                      │
│                                                             │
│  • pages/[...slug].tsx   — single catch-all route           │
│  • Masonry / SearchBar / PinterestModal / settings/         │
│  • TanStack Query hooks for image, tag, root, search state  │
│  • Tauri event subscription for indexing-progress           │
│  • Optional PerfOverlay (--profiling only)                  │
└──────────────────────────────┬──────────────────────────────┘
                               │ Tauri IPC (typed ApiError)
┌──────────────────────────────▼──────────────────────────────┐
│                       Rust Backend                          │
│                                                             │
│  • commands/   — 26 Tauri commands, grouped by concern       │
│  • db/         — SQLite (WAL, writer + read-only secondary) │
│  • indexing.rs — single-flight pipeline (scan → thumbs →    │
│                  encoders → cosine cache repopulation)       │
│  • thumbnail/  — JPEG scaled IDCT + Lanczos3 (fast_image..)  │
│  • watcher.rs  — notify-debouncer-mini, 5 s debounce        │
│  • similarity_and_semantic_search/                          │
│    ├── encoders/   CLIP / DINOv2 / SigLIP-2 (image + text)  │
│    ├── cosine/     per-encoder caches + RRF fusion          │
│    └── ort_session — shared M2-tuned Session builder         │
│  • model_download.rs — HuggingFace first-launch download    │
│  • paths.rs    — platform app-data dir resolution           │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                       Local Storage                         │
│                                                             │
│  <app-data-dir>/                                            │
│    images.db                  — metadata, tags, notes,      │
│                                  embeddings, roots, meta    │
│    thumbnails/<root>/...      — 400×400 JPEG previews       │
│    models/                    — CLIP, DINOv2, SigLIP-2      │
│    cosine_cache.bin           — persistent ranking cache    │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                       ONNX Runtime                          │
│                                                             │
│  • CLIP ViT-B/32 vision + text (OpenAI, 512-d)              │
│  • DINOv2-Base vision (Meta, 768-d)                         │
│  • SigLIP-2 Base 256 vision + text (Google, 768-d shared)   │
│                                                             │
│  CPU on macOS (CoreML produces runtime errors for these     │
│  graphs); CUDA on non-macOS with CPU fallback.              │
└─────────────────────────────────────────────────────────────┘
```

For the full structural map — module-by-module responsibilities, table layouts, lifecycle diagrams, command surfaces — see [`context/architecture.md`](./context/architecture.md). For per-subsystem detail, see [`context/systems/`](./context/systems/).

---

## Design principles

- **Local-first** — all computation, storage, and inference runs on your machine. No cloud dependencies, no API keys, no network required after first-launch model download.
- **Privacy by construction** — original images are never modified or uploaded; thumbnails, notes, and embeddings are derived locally and stored in a local SQLite database.
- **Performance at scale** — thumbnail caching, embedding precomputation, cosine cache persistence, and parallel encoder execution mean the UI stays fast regardless of library size.
- **Offline ML inference** — every encoder runs entirely via ONNX Runtime. No Python, no external ML service, no GPU required (CUDA used on non-macOS when available).
- **Modularity and toggleability** — encoders are swappable; per-encoder toggles let you enable any subset without rebuilding. The fusion ranker adapts to whichever encoders are active.
- **Separation of concerns** — React frontend, Tauri IPC layer, Rust backend logic, and SQLite persistence are cleanly separated and independently testable. 125 Rust unit tests + 62 Vitest tests gate every change.
- **Observability when you need it, zero overhead when you don't** — the profiling layer is opt-in via a CLI flag and produces a structured markdown report on exit.

---

## Tech stack

| Layer | Tools |
|-------|-------|
| Desktop shell | Tauri 2 |
| Frontend | React 19, Vite 7, TanStack Query 5, Tailwind CSS 4, Radix UI, framer-motion, cmdk, lucide-react |
| Frontend testing | Vitest 4, Testing Library, JSDOM |
| Backend | Rust 2021 edition |
| Persistence | SQLite (WAL mode) via `rusqlite` |
| Image I/O | `image-rs`, `jpeg-decoder`, `fast_image_resize` |
| ML runtime | `ort = 2.0.0-rc.10` (ONNX Runtime bindings) |
| Tokenisation | HuggingFace `tokenizers` (BPE for CLIP, SentencePiece for SigLIP-2) |
| Filesystem watcher | `notify` + `notify-debouncer-mini` |
| Profiling | `tracing` + custom `PerfLayer`, `sysinfo` for RSS/CPU sampler |
| Concurrency | `rayon` for parallel encoder execution |

---

## Running locally

```bash
# Clone the repository
git clone https://github.com/Capataina/PinterestStyleImageBrowser
cd PinterestStyleImageBrowser

# Install frontend dependencies
npm install

# Run in development mode
npm run tauri dev

# Run in profiling mode (writes a markdown report on exit)
npm run tauri dev -- -- --profiling
```

To build a release bundle:

```bash
npm run tauri build
```

To run the test suites:

```bash
npm test                    # Vitest (frontend)
cd src-tauri && cargo test  # cargo (backend)
```

No API keys required. No internet connection required after the first launch. Encoder models are downloaded once from HuggingFace on first launch (~2.5 GB total) and cached in your platform's app-data directory.

### App data location

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/com.ataca.image-browser/` |
| Linux | `~/.local/share/com.ataca.image-browser/` |
| Windows | `%APPDATA%\com.ataca.image-browser\` |

Override with the `IMAGE_BROWSER_DATA_DIR` environment variable. There is no separate dev-vs-release path — both write to the same location.

---

## Project documentation

| Folder | Purpose |
|--------|---------|
| [`README.md`](./README.md) | This file — project intent, features, usage, high-level architecture |
| [`context/`](./context/) | Implementation memory — architecture map, per-subsystem docs, durable design notes, active plan files, research references |
| [`learning/`](./learning/) | Teaching material covering the project and surrounding domain (CLIP, DINOv2, SigLIP-2, RRF, Tauri, ONNX Runtime, …) |

---

## Summary

Image Browser is a local-first desktop application that brings intelligent image search to personal libraries without any cloud dependency. It delivers end-to-end product across a Rust backend, Tauri 2 desktop shell, React 19 frontend, SQLite persistence layer, and ONNX Runtime inference pipeline — with three independently-trained vision encoders (CLIP, DINOv2, SigLIP-2) fused via Reciprocal Rank Fusion to power both image-to-image and natural-language image search, all running entirely offline on consumer hardware.
