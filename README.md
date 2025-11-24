# Local-First Pinterest-Style Image Browser

A lightweight, high-performance desktop application for browsing large local image collections using a modern Pinterest-style masonry layout, manual tagging, a built-in search bar, visual similarity search, semantic search, and an optional timed slideshow mode.

Built with Rust, Tauri, and React. Designed to handle thousands of images smoothly with complete offline privacy.

**Use it for:**

- Organising personal photos
- Browsing inspiration packs or mood boards
- Managing art and design reference libraries
- Searching images by tags or meaning
- Finding visually similar images
- Running slideshows for study or review

---

## Overview

File explorers struggle with large, nested image collections. This app improves the experience with:

- **Pinterest-style masonry grid**
- **Manual tagging system**
- **Universal search bar** (tags initially, semantic search later)
- **Semantic search** via CLIP (e.g. "forest", "portrait", "cat", "architecture")
- **Similarity search** ("show similar images")
- **Slideshow viewer**
- **Fully local thumbnail and embedding database**
- **No cloud, no accounts, no external services**

---

## Architecture Overview

### Frontend

- React SPA inside Tauri
- Masonry grid feed
- Tag chips and tag editor
- Search bar (tag search initially, semantic later)
- "View Similar" page
- Slideshow viewer

### Backend (Rust)

- Tauri IPC commands
- Recursive folder scanning
- Thumbnail generation
- SQLite database:
  - Image metadata
  - Tags
  - Embeddings
- ONNX Runtime for CLIP image and text encoders
- Cosine similarity engine

### Data

- SQLite stored locally
- Thumbnails cached in `/thumbs/`
- Embeddings stored as BLOB
- Original images untouched

---

## Roadmap and Milestones

- [ ] **Milestone 1** – Basic Folder Viewer  
       Minimal grid, recursive scanning

- [ ] **Milestone 2** – Database and Thumbnails  
       SQLite and thumbnail caching

- [ ] **Milestone 3** – Masonry UI, Slideshow, Search Bar, and Manual Tagging  
       Pinterest-style feed, search bar UI (tag search), tagging, slideshow

- [ ] **Milestone 4** – Similarity Search  
       CLIP image embeddings → "View Similar Images"

- [ ] **Milestone 5** – Semantic Search  
       Enhance the existing search bar with CLIP text embeddings

- [ ] **Milestone 6** – Reserved for Future Features  
       (Boards, auto-tagging, video/GIF support, etc.)

---

## Milestone 1: Basic Folder Viewer

**Goal:** Display images from disk in a basic grid

### Backend

- [x] Set up Tauri project
- [x] `scan_folder(path)` (recursive image lookup)

### Frontend

- [x] Simple grid showing full-resolution images
- [ ] Folder picker or fixed path

**Deliverable:** Basic viewer

---

## Milestone 2: Database and Thumbnails

**Goal:** Persistent local library with fast thumbnail browsing

### Backend

- [x] SQLite integration
- [x] `images` table with metadata
- [ ] Thumbnail generation stored in `/thumbs/`
- [ ] `list_images(offset, limit)`

### Frontend

- [ ] Render thumbnails
- [ ] Pagination → infinite scroll preparation

**Deliverable:** Thumbnail-based image library

---

## Milestone 3: Masonry UI, Slideshow, Search Bar, and Manual Tagging

**Goal:** Deliver the core UX (browsing, searching, tagging, and viewing)

### Masonry Grid (Frontend)

- [x] Infinite scroll masonry layout
- [x] Hover interactions
- [x] Click to inspect image

### Manual Tagging

#### Backend

- [ ] `tags` table
- [ ] `image_tags` table
- [ ] Commands:
  - `add_tag(image_id, name)`
  - `remove_tag(image_id, name)`
  - `list_tags(image_id)`
  - `filter_by_tag(name)`

#### Frontend

- [ ] Add/remove tag UI
- [ ] Tag chips under images
- [ ] Tag filtering via search bar

### Search Bar (Phase 1: Tag/Filename Search)

- [ ] Global search input
- [ ] Filter image results:
  - Tags
  - Filenames (optional)
  - Pack roots (optional)

### Slideshow Mode

- [ ] Fullscreen slideshow
- [ ] Timer (N seconds per image)
- [ ] Next/Previous
- [ ] Works on:
  - Main feed
  - Tag-filtered feed
  - Search-filtered feed

**Deliverable:** Fully functional browsing UX with search, tags, and slideshow

---

## Milestone 4: Similarity Search

**Goal:** "View Similar Images" using CLIP image embeddings

### Backend

- [ ] Add `embeddings` table
- [x] Integrate CLIP image encoder
- [ ] Generate embeddings on indexing
- [ ] Normalise vectors
- [ ] Load into memory on startup
- [ ] `get_similar(image_id, limit)`

### Frontend

- [ ] "View Similar" button on image click
- [ ] Masonry layout for similarity results
- [ ] Slideshow support

**Deliverable:** Instant visual similarity search across the entire library

---

## Milestone 5: Semantic Search

**Goal:** Upgrade the existing search bar to support natural language search

### Backend

- [ ] Integrate CLIP text encoder
- [ ] Replace/extend search logic:
  - `search_images(query_text, limit)`
  - Compare text embedding with every image embedding
- [ ] Cosine similarity ranking

### Frontend

- [ ] Search bar stays the same
- [ ] Switch result rendering to semantic mode
- [ ] Tag search and semantic search coexist naturally

**Example Queries:**

- "skull"
- "female portrait"
- "forest path"
- "dark cinematic lighting"
- "neon cityscape"
- "dynamic gesture pose"

**Deliverable:** A fully intelligent search system without relying on manual tags

---

## Milestone 6: Reserved for Future Extensions

**Possible expansions:**

- Auto-tagging with CLIP
- GIF/video embedding (frame sampling)
- Saved collections and boards
- Drag-and-drop import
- Cluster-based discovery
- NSFW filters
- Offline embeddings update

---

## Summary

This app provides a refined, private, and highly capable way to browse large local image libraries:

- Modern Pinterest-style grid
- Manual tagging and tag-based filtering
- Early search bar (tag and name search)
- CLIP-powered similarity search
- CLIP-powered semantic search
- Slideshow mode
- Efficient Rust backend
- No cloud, fully local

A flexible, scalable foundation for any image-heavy workflow.
