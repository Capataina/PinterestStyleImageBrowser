---
source_type: shipped-project
date_published: 2025-09
hype_score: 1
---

# Firefox Translate / Local AI Runtime (Mozilla)

## Source reference

- Mozilla Hacks: https://hacks.mozilla.org/2024/05/experimenting-with-local-alt-text-generation-in-firefox-nightly/
- Mozilla Blog: https://blog.mozilla.org/en/firefox/firefox-ai/speeding-up-firefox-local-ai-runtime/

## Claim summary

Firefox ships a local-AI runtime that embeds ONNX Runtime + Transformers.js. Does on-device translation (Bergamot + WASM) and local alt-text generation. Native ONNX backend for ORT-Web gives 2-10× speedup vs WASM-only.

## Relevance to our project

A1 + A2: A *production-shipped* example of "ONNX Runtime + on-device ML" inside a major piece of consumer software (Firefox is on hundreds of millions of machines). Validates the project's local-first ONNX-Runtime stack at scale.

## Specific takeaways

- Mozilla's choice to embed ONNX Runtime over training a custom inference engine is a strong durability signal for the underlying runtime.
- Firefox's local alt-text feature is *image-encoder-driven* — same primitive the project uses for similarity / semantic search. This is structurally relevant.
- The "20MB language model, runs offline" aesthetic matches the project's design.

## Hype indicators

None — Mozilla engineering blog with code references.
