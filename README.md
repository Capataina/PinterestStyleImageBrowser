# Image Browser

> A local-first desktop application for browsing and searching large image collections — Pinterest-style masonry layout, manual tagging, CLIP-powered visual similarity search, and natural language semantic search, all running entirely on your machine with no cloud, no accounts, and no external services.

---

## Why Image Browser exists

File explorers are built for files, not images. When you have thousands of images across nested folders — reference collections, inspiration boards, photography libraries, art assets — navigating them with a file explorer means clicking through directories one by one with no way to search by meaning, find visually similar images, or organise by anything other than filename or date.

Image Browser solves this by treating your local image library as a first-class collection: thumbnailed, indexed, tagged, and searchable — both by text labels and by semantic meaning. You can type "dark cinematic lighting" or "forest path at dusk" and find matching images without having manually tagged them. You can click any image and instantly surface the visually similar ones from across your entire library.

Everything runs locally. Embeddings are generated on your machine using ONNX Runtime. Nothing leaves your computer.

---

## What Image Browser does

- **Pinterest-style masonry grid** — infinite scroll layout that handles thousands of images without performance degradation
- **Manual tagging** — add and remove tags per image, filter the entire library by tag
- **Visual similarity search** — click any image to find the most visually similar ones across the full library using CLIP image embeddings
- **Semantic search** — type natural language queries ("skull", "neon cityscape", "dynamic pose") and retrieve matching images using CLIP text embeddings compared against stored image embeddings
- **Slideshow mode** — fullscreen slideshow across any view: main feed, tag-filtered results, or search results
- **Fully offline** — all computation, indexing, and inference runs locally with no network requirements

---

## Architecture

Image Browser runs as a local Tauri desktop application:

```
┌─────────────────────────────────────────┐
│            React Frontend               │
│  Masonry grid, search bar, tag UI       │
│  Slideshow, similarity results view     │
└──────────────────┬──────────────────────┘
                   │ Tauri IPC
┌──────────────────▼──────────────────────┐
│            Rust Backend                 │
│  Folder scanning, thumbnail generation  │
│  Tag management, search logic           │
│  CLIP embedding generation              │
│  Cosine similarity engine               │
└──────────────────┬──────────────────────┘
                   │
┌──────────────────▼──────────────────────┐
│           Local Storage                 │
│  SQLite: image metadata, tags,          │
│  embeddings                             │
│  Thumbnail cache on disk                │
└──────────────────┬──────────────────────┘
                   │
┌──────────────────▼──────────────────────┐
│           ONNX Runtime                  │
│  CLIP image encoder (local inference)   │
│  CLIP text encoder (local inference)    │
└─────────────────────────────────────────┘
```

---

## Design Principles

- **Local-first**: all computation, storage, and inference runs on your machine — no cloud dependencies, no API keys, no network required
- **Privacy by construction**: original images are never modified or uploaded; thumbnails and embeddings are derived locally and stored in a local SQLite database
- **Performance at scale**: thumbnail caching and embedding precomputation mean the UI stays fast regardless of library size
- **Offline ML inference**: CLIP embeddings are generated and compared entirely via ONNX Runtime — no Python, no external ML service, no GPU required
- **Separation of concerns**: React frontend, Tauri IPC layer, Rust backend logic, and SQLite persistence are cleanly separated and independently testable

---

## Roadmap

- [x] Milestone 1: Basic Folder Viewer
- [x] Milestone 2: Database & Thumbnail System
- [x] Milestone 3: Masonry UI, Tagging & Slideshow
- [x] Milestone 4: Visual Similarity Search
- [ ] Milestone 5: Semantic Search
- [ ] Milestone 6: Future Extensions

---

## 📍 Milestones

---

### Milestone 1 — Basic Folder Viewer ✅

> **Goal**: Display images from a local folder in a basic grid — the minimal foundation proving the Tauri + Rust + React stack works end to end

#### Core Concept
Before any intelligence is added, the application needs to do one thing: open a folder and display its images. This milestone establishes the full stack — Tauri desktop shell, Rust backend command handling, React frontend rendering — so every subsequent milestone builds on proven infrastructure.

---

- [x] Tauri project set up and running locally
- [x] Recursive folder scan finding all image files in a selected directory
- [x] Basic grid displaying full-resolution images
- [x] Folder picker allowing the user to select any directory

**Exit criteria**: Application launches, user selects a folder, images display in a grid

---

### Milestone 2 — Database & Thumbnail System ✅

> **Goal**: Replace full-resolution grid with a fast thumbnail-based library backed by persistent local storage

#### Core Concept
Loading full-resolution images for every item in a large library is unusable at scale. This milestone introduces SQLite for persistent image metadata and a thumbnail generation pipeline that creates small cached previews on first scan. Subsequent launches are fast because thumbnails already exist — only new images need processing.

---

- [x] SQLite database integrated with image metadata table
- [x] Thumbnail generation on first index, cached to disk
- [x] Paginated image loading preparing for infinite scroll
- [x] Thumbnails rendered in frontend instead of full-resolution images

**Exit criteria**: Library of thousands of images opens quickly using cached thumbnails; rescan only processes new additions

---

### Milestone 3 — Masonry UI, Tagging & Slideshow ✅

> **Goal**: Deliver the full core browsing experience — Pinterest-style layout, manual organisation via tags, and fullscreen slideshow

#### Core Concept
A grid of equal-sized thumbnails wastes space and loses the visual character of images with different aspect ratios. Masonry layout preserves each image's proportions while packing them efficiently. On top of this, manual tagging gives users a way to organise their library, and slideshow mode provides a distraction-free viewing experience across any filtered view.

---

- [x] Infinite scroll masonry layout preserving image aspect ratios
- [x] Click to inspect individual image
- [x] Tag management — add and remove tags per image
- [x] Tag filtering — search bar filters library to images matching selected tags
- [x] Fullscreen slideshow mode working across main feed and filtered views
- [x] Hover interactions and smooth scroll behaviour

**Exit criteria**: User can browse a large library in masonry layout, tag images, filter by tag, and launch a slideshow

---

### Milestone 4 — Visual Similarity Search ✅

> **Goal**: Click any image and instantly surface the most visually similar images from across the entire library using CLIP embeddings

#### Core Concept
Visual similarity search is the technically interesting core of the project. Every image in the library is passed through a CLIP image encoder to produce a 512-dimensional embedding vector that captures its visual and semantic content. These embeddings are stored in SQLite and loaded into memory at startup. When the user clicks "View Similar", the clicked image's embedding is compared against all others using cosine similarity, and the closest matches are returned ranked by similarity score. All of this runs locally via ONNX Runtime — no GPU required, no external service.

---

- [x] CLIP image encoder integrated via ONNX Runtime
- [x] Embeddings generated for all images on indexing and stored in SQLite
- [x] Embedding vectors normalised and loaded into memory on startup
- [x] Cosine similarity engine comparing a query embedding against the full library
- [x] "View Similar" button on image inspect view
- [x] Similarity results displayed in masonry layout with slideshow support

**Exit criteria**: Clicking "View Similar" on any image returns visually and semantically similar images ranked by similarity score

---

### Milestone 5 — Semantic Search

> **Goal**: Upgrade the search bar to support natural language queries using CLIP text embeddings matched against stored image embeddings

#### Core Concept
CLIP was trained on image-text pairs, meaning its image and text encoders share the same embedding space. A text query like "dark cinematic lighting" produces an embedding that sits close in the vector space to images depicting dark, cinematic scenes — even if those images have never been manually tagged. This milestone adds a CLIP text encoder to the pipeline, so the search bar compares a text embedding against all stored image embeddings and returns semantically matching results ranked by cosine similarity.

---

- [x] CLIP text encoder integrated via ONNX Runtime
- [x] Search bar switches to semantic mode when no exact tag match is found
- [x] Text query encoded to embedding and compared against all stored image embeddings
- [x] Results ranked by cosine similarity and displayed in masonry layout
- [x] Tag search and semantic search coexist naturally — exact tag matches take priority

**Exit criteria**: Typing "forest path", "skull", or "neon cityscape" into the search bar returns semantically matching images from an untagged library

---

### Milestone 6 — Future Extensions

> Possible directions once the core feature set is complete

- Auto-tagging via CLIP zero-shot classification
- Saved collections and boards
- Drag-and-drop folder import
- GIF and video support via frame sampling
- Cluster-based discovery — group visually similar images automatically
- Embedding update for newly added images without full rescan

---

## Running Locally

```bash
# Clone the repository
git clone https://github.com/Capataina/PinterestStyleImageBrowser
cd PinterestStyleImageBrowser

# Install frontend dependencies
npm install

# Run in development mode
npm run tauri dev
```

No API keys required. No internet connection required. CLIP models are bundled and run locally via ONNX Runtime.

---

## Summary

Image Browser is a local-first desktop application that brings intelligent image search to personal libraries without any cloud dependency. It demonstrates end-to-end product delivery across a Rust backend, Tauri desktop shell, React frontend, SQLite persistence layer, and ONNX Runtime inference pipeline — with CLIP-powered visual similarity and semantic search running entirely offline on consumer hardware.
