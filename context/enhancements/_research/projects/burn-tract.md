---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# Burn (Tracel) and tract (Sonos)

## Source reference

- Burn GitHub: https://github.com/tracel-ai/burn
- tract GitHub: https://github.com/sonos/tract
- Burn ONNX import: https://github.com/tracel-ai/burn-onnx/

## Claim summary

Two more pure-Rust ML inference frameworks. **Burn** is a tensor-library + DL framework with ONNX import (converts ONNX → Burn Rust code). Tracel-funded, growing community. **tract** is Sonos's lightweight no-deps inference engine; passes ~85% of ONNX backend tests including ResNet50, SqueezeNet, VGG19.

## Relevance to our project

A1: Two more Rust-native inference paths. Both are *plug-and-play candidates* if the project ever wants to move off `ort`'s C++ runtime for stack purity. tract specifically is the lightweight option; Burn is the full-featured one.

## Specific takeaways

- tract has shipped in production at Sonos (smart-speaker NLU). Strong durability signal.
- Burn is more research-y but has the better long-term roadmap (training, GPU backends).
- Both can replace `ort` behind a trait — the project's encoder code is tightly coupled to `ort` types now, but a clean abstraction would unlock either.
- The recommendation downstream is *not* "switch to Candle / Burn / tract" — it's "abstract the encoder behind a trait so the choice is reversible".

## Hype indicators

None — both are substantive shipping projects.
