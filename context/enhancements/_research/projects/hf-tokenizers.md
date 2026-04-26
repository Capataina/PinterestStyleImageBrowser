---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# HuggingFace `tokenizers` (Rust)

## Source reference

- GitHub: https://github.com/huggingface/tokenizers
- Crates.io: https://crates.io/crates/tokenizers

## Claim summary

HF's flagship tokenisation library. **Originally written in Rust**; exposes Python and Node bindings as wrappers. Supports BPE, WordPiece, Unigram, SentencePiece. Tokenises 1 GB of text in <20 seconds.

## Relevance to our project

A1: The project explicitly chose **NOT** to depend on `tokenizers` (per Decision D6) because the original `tokenizers` crate had a transitive C dependency for SentencePiece — the project's pure-Rust WordPiece reimplementation is ~170 lines. The user's framing in the vault: "Tokenizer is now implemented in pure Rust within encoder_text.rs. No external tokenizer dependency needed!"

The current `tokenizers` crate has reduced its C dependencies. The trade-off is now: ~170 lines of bespoke code vs adopting the canonical implementation. For the audience, this is a *credibility* decision — either approach is defensible, and the project's choice of bespoke reimplementation actually shows higher craftsmanship.

## Specific takeaways

- The Image Browser's pure-Rust WordPiece is *less full-featured* than HF `tokenizers` (no full normaliser chain, no punctuation handling, no BPE) — but covers the multilingual-CLIP path correctly.
- A potential portfolio direction: contribute back the lessons learned (e.g., a "minimal-WordPiece" example) to HF's docs or as an OSS gist.
- License (HF tokenizers): Apache 2.0.

## Hype indicators

Mild — HF has marketing voice but the library is technical reality.
