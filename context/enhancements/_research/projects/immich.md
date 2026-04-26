---
source_type: shipped-project
date_published: 2026-01
hype_score: 2
---

# Immich

## Source reference

- GitHub: https://github.com/immich-app/immich
- Docs: https://docs.immich.app/features/searching/
- Site: https://immich.app

## Claim summary

Self-hosted high-performance photo/video management (Google-Photos alternative). 80k+ GitHub stars. Uses CLIP for semantic search via a separate Python ML service running on TensorFlow + the VectorChord PostgreSQL extension. Includes facial recognition (people clusters), AI search, mobile apps with background upload.

## Relevance to our project

A2 (Local-first) + A4 (Retrieval): The most directly comparable project in the space. Immich is server+cloud-style (self-hosted, runs on your own server); Image Browser is desktop-first (runs on your machine, no server). The architectural comparison is informative — both use CLIP, both index locally, the difference is single-machine vs server.

A3 (Tauri+React): Immich is *not* Tauri (it's React + Node + Python server). The project's "Tauri-2 + pure Rust ML inference" stack is a *different* choice — a clean differentiator for the audience that wants no Python in the stack.

## Specific takeaways

- Immich's "you can search 'sunset' or 'beach' or 'birthday party'" is exactly the project's semantic-search promise — but with a server.
- 80k stars is the upper bound on attention this category can attract.
- The project's pure-Rust ML inference (no Python service) is a *strong* differentiator vs Immich. Worth marking explicitly.
- Immich's facial-recognition feature is the natural next-step in this project's roadmap (M6 territory).

## Hype indicators

Mild — Immich has heavy community marketing volume but the underlying code is real and shipping.
