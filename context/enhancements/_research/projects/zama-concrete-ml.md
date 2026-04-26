---
source_type: shipped-project
date_published: 2026-04
hype_score: 2
---

# Zama Concrete-ML

## Source reference

- GitHub: https://github.com/zama-ai/concrete-ml
- Docs: https://docs.zama.org/concrete-ml

## Claim summary

Privacy-preserving ML framework built on Concrete (Zama's CKKS/TFHE/BFV core). Compiles scikit-learn / PyTorch models to FHE-evaluable equivalents. Quantizes models to integer-only inference (FHE constraint). Use cases: encrypted sentiment analysis, encrypted tree-based classifiers.

## Relevance to our project

A2: An additional substantive backing for Zama / TFHE-rs as the canonical "FHE in code" stack. Concrete-ML is Python-facing, but its underlying compiler (Concrete) is shared with TFHE-rs. The patterns transfer.

A4: For the encrypted-vector recommendation, Concrete-ML demonstrates that FHE-on-ML is now usable enough to have a high-level abstraction layer over it. This is much more substantive than "FHE is theoretically interesting".

## Specific takeaways

- License: Zama's modified BSD 3-Clause Clear (free for non-commercial use; commercial requires patent licence).
- The FHE-CLIP pipeline would be: train CLIP on plaintext (already done by OpenAI/Apple/Google) → quantise weights → wrap inference in TFHE-rs / Concrete-ML primitives.
- The realistic FHE-CLIP path is *encrypt the query embedding only*, not the model weights — encrypted query against plaintext index.

## Hype indicators

Mild — Zama markets aggressively but the codebase is real.
