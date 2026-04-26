---
source_type: shipped-project
date_published: 2025-04-26
hype_score: 2
---

# TFHE-rs (Zama)

## Source reference

- GitHub: https://github.com/zama-ai/tfhe-rs
- Docs: https://docs.zama.org/tfhe-rs/get-started/benchmarks
- Announcement: https://www.zama.org/post/announcing-tfhe-rs

## Claim summary

Pure-Rust implementation of the TFHE (CGGI) FHE scheme. Supports boolean and integer arithmetic over encrypted data with up to 8 bits of message space (chained for higher precision via PBS). AVX-512 acceleration, GPU acceleration on H100 (2.4× speedup with classical PBS).

## Relevance to our project

A2 (Local-first / privacy-eng): The user has already drafted a vault Work file proposing encrypted vector search using TFHE-rs (`Projects/Image Browser/Work/Encrypted Vector Search.md`). TFHE-rs is the named library; the question is durability and feasibility.

A1: TFHE-rs is *the* canonical "FHE in pure Rust" library. Maintained by a $73M-funded company (Zama), with consistent releases and active GitHub presence.

## Specific takeaways

- Stars: ~1.5k, contributors: 50+, recent commits: weekly.
- License: BSD 3-Clause Clear (Zama's modified BSD, FHE-specific).
- Bench example: scalar AND on encrypted bool ~6.4ms on AVX-512 CPU; encrypted u8 add ~13ms; 64-bit comparisons in the 100ms range.
- The cosine-similarity inner-product is *expensive* under TFHE — multiplication over high-dim vectors is the dominant cost. The project's vault note already acknowledges 4-5 orders of magnitude slowdown vs plaintext.
- For 512-d CLIP × N-image inner products, the realistic envelope is maybe single-image-per-second on CPU. The Pinterest 7-tier mode is *not* tractable; only single-pair similarity and small top-K linear scans are.

## Hype indicators

Mild. Zama markets aggressively but the underlying code is open, reproducible, benchmarked.
