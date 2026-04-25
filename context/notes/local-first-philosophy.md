# local-first-philosophy

## Current Understanding

Image Browser is local-first by construction. Every piece of computation, persistence, and ML inference runs on the user's machine. There are no API keys, no cloud dependencies, no network calls in the hot path.

Concrete manifestations:

- SQLite file lives next to the app binary (`images.db`).
- CLIP embeddings are generated locally via ONNX Runtime; the models are user-supplied (`models/*.onnx`).
- Thumbnails are cached on local disk (`.thumbnails/`).
- The Tauri config disables CSP (`csp: null`) and grants asset-protocol scope to the entire filesystem (`scope: ["**"]`) — fine for a single-user local tool, dangerous for any multi-user deployment.
- Original images are never modified, copied, or uploaded.

## Rationale

The app exists to handle personal image libraries — collections that are private by nature (reference boards, personal photography, downloaded inspiration). Cloud sync would defeat the purpose. The README states this explicitly: "all computation, storage, and inference runs on your machine — no cloud dependencies, no API keys, no network required."

This shapes engineering decisions:

- ONNX Runtime over a hosted Embeddings API.
- SQLite over Postgres.
- Tauri (native shell) over Electron-with-server.
- Pure-Rust WordPiece tokenizer over the `tokenizers` crate's C dependencies.

## Guiding Principles

- New features should default to local. Anything that needs the network is suspect.
- Privacy is by construction, not by configuration. The user should not need to opt in to local-first; opting *out* should require deliberate work.
- Performance is a local concern. Any architectural choice that degrades single-user performance to gain multi-user scale (e.g., adding a server hop) is the wrong direction.
- ML inference belongs locally. The pure-Rust tokenizer + ONNX Runtime stack proves this is feasible on consumer CPUs.

## What Was Tried

The original memory-bank planning notes (now deleted as misleading) discussed cloud-sync features and a Zustand-based state model that anticipated multi-user concerns. Neither shipped. The decision to go fully local was made early and has not been revisited.
