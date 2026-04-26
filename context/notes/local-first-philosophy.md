# local-first-philosophy

## Current Understanding

Image Browser is local-first by construction. Every piece of computation, persistence, and ML inference runs on the user's machine. The single network operation is the **first-launch model download from HuggingFace** (~1.15 GB total) — once the models are on disk, the app never reaches out again unless the user manually deletes them.

Concrete manifestations:

- SQLite file lives in `Library/images.db` (in-repo dev folder, platform app-data dir in release).
- CLIP embeddings are generated locally via ONNX Runtime; the models are auto-downloaded on first launch (Phase 4b).
- Thumbnails are cached on local disk under `Library/thumbnails/root_<id>/`.
- The Tauri config disables CSP (`csp: null`) and grants asset-protocol scope to the entire filesystem (`scope: ["**"]`) — fine for a single-user local tool, dangerous for any multi-user deployment. Documented as a hardening target in `enhancements/recommendations/08-tauri-csp-asset-scope-hardening.md`.
- Original images are never modified, copied, or uploaded.
- Filesystem watcher (`notify-debouncer-mini`) monitors the user's library locally; no cloud sync.
- Profiling (`--profile` mode) writes diagnostics to `Library/exports/perf-<ts>/` — never sent anywhere.

## Rationale

The app exists to handle personal image libraries — collections that are private by nature (reference boards, personal photography, downloaded inspiration). Cloud sync would defeat the purpose. The README states this explicitly: "all computation, storage, and inference runs on your machine — no cloud dependencies, no API keys, no network required."

This shapes engineering decisions:

- ONNX Runtime over a hosted Embeddings API.
- SQLite over Postgres.
- Tauri (native shell) over Electron-with-server.
- Pure-Rust WordPiece tokenizer over the `tokenizers` crate's C dependencies.
- HuggingFace download is opt-in by use — if the user supplies the model files manually (or never uses semantic search), the app stays fully offline.

## Guiding Principles

- New features should default to local. Anything that needs the network is suspect.
- Privacy is by construction, not by configuration. The user should not need to opt in to local-first; opting *out* should require deliberate work.
- Performance is a local concern. Any architectural choice that degrades single-user performance to gain multi-user scale (e.g., adding a server hop) is the wrong direction.
- ML inference belongs locally. The pure-Rust tokenizer + ONNX Runtime stack proves this is feasible on consumer CPUs and GPUs (CoreML on macOS, CUDA elsewhere).
- The model download is the only acceptable network call — and it's a one-time setup operation, not a runtime dependency. Future re-download flow (triggered by `isMissingModelError(e)`) would be the second acceptable case.

## What Was Tried

The original memory-bank planning notes (now deleted as misleading) discussed cloud-sync features and a Zustand-based state model that anticipated multi-user concerns. Neither shipped. The decision to go fully local was made early and has not been revisited.

## Trigger to revisit

If the project ever pivots to multi-user or hosted-inference, this principle would need explicit revisitation. Today nothing in the roadmap or README suggests that direction; the project-enhancement skill's `enhancements/recommendations/` artefacts (encrypted vector search, OTLP export, etc.) all maintain the local-first stance with explicit local primacy.

## Cross-references

- `systems/paths-and-state.md` § Library/ layout (where everything lives)
- `systems/model-download.md` (the one network operation)
- `enhancements/recommendations/07-encrypted-vector-search-mvp.md` (a stronger version of local-first via FHE)
- `enhancements/recommendations/08-tauri-csp-asset-scope-hardening.md` (the security flip-side)
