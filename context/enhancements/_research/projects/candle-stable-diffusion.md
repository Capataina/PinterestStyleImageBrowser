---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# Stable Diffusion in Rust (Candle + ONNX Runtime)

## Source reference

- Candle SD example: https://github.com/huggingface/candle/tree/main/candle-examples/examples/stable-diffusion
- HF stable-diffusion crate: https://crates.io/crates/stable-diffusion
- Microsoft DirectML SD: https://devblogs.microsoft.com/dotnet/generate-ai-images-stable-diffusion-csharp-onnx-runtime/

## Claim summary

Stable Diffusion 1.5 / 2.1 / SDXL / Turbo all run via Candle's pure-Rust path or via ONNX Runtime. The Candle path supports CUDA, Metal, ONNX, CPU backends. Demonstrates that "modern generative ML in Rust without Python" is a real production pattern.

## Relevance to our project

A1: Strong durability signal for the project's stack choice (Rust + ONNX). If SD-XL runs in Rust, CLIP-ViT-B/32 inference is trivially within scope.

A4: A *future* extension direction: add a "generate similar image" feature using SD with a CLIP-conditioned prompt derived from the selected image. Far outside current scope but a credible "where could this go" arc.

## Specific takeaways

- Confirms the entire Diffusers / Transformers pipeline can run in Rust without Python.
- The Candle path is now the canonical way to run modern HF models in Rust without a tokio-runtime requirement.

## Hype indicators

Mild.
