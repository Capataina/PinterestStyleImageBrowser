---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# ort (pyke) — ONNX Runtime Rust bindings

## Source reference

- Site: https://ort.pyke.io/
- GitHub: https://github.com/pykeio/ort
- Crates.io: https://lib.rs/crates/ort

## Claim summary

The Rust gateway to ONNX Runtime. Currently at **2.0.0-rc.12** (production-ready, not API-stable). CoreML EP is feature-flagged; CUDA, DirectML, TensorRT, OpenVINO all available. The project pins to `2.0.0-rc.10`.

## Relevance to our project

A1: The single most important dependency in the project's ML path. Its durability is the project's durability for the ML stack.

## Specific takeaways

- The `2.0.0-rc.X` chain has been ongoing for over a year — a 2.0 stable release is widely expected but has not landed. The project is early on this track and the API has been stable across the rc series.
- CoreML EP support is real but has gaps — confirmed in the project's own commits `90a3842` and `2775c9f` ("Disable CoreML for the image encoder too — runtime inference errors", "Skip CoreML on the text encoder (transformer ops poorly supported)"). The project documents this honestly.
- Per-EP fallback semantics: if one EP fails to load a node, ort's session-level fallback loads the model on CPU. The project relies on this.
- Maintainer (decahedron @ pyke) is responsive on GitHub Issues; the project is well-supported.

## Hype indicators

None — technical infrastructure project, no marketing voice.
