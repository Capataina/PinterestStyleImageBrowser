---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# Candle (Hugging Face)

## Source reference

- GitHub: https://github.com/huggingface/candle

## Claim summary

Hugging Face's pure-Rust ML framework. Inference-first (training experimental). Compiles to single-MB binaries; first-class WebAssembly support. Whisper, Llama-2, and other major models run in-browser via WASM.

## Relevance to our project

A1: The leading Rust ML framework alternative to `ort`. Used by NeuroDrive (per the user's vault) for PPO. The project picked `ort` (Decision D5) because it's more mature for ONNX and CUDA — and that's correct. But Candle's WASM story enables a *future* path where the encoder runs in the WebView itself, eliminating an IPC roundtrip.

## Specific takeaways

- Candle could host CLIP via Rust-native model code, eliminating the ONNX dependency.
- WASM build is the differentiator — opens a "browser-only" deployment path that ONNX-Runtime can't easily match.
- Switching from `ort` to Candle is **commitment-grade** (replaces the entire inference stack).
- Trade-off: Candle's CUDA story is less mature than `ort`'s. For this project (which targets desktop / consumer hardware including macOS/CoreML), the trade-off is real.

## Hype indicators

Mild — Hugging Face has marketing voice, but Candle is substantive code with a $4B-valued sponsor.
