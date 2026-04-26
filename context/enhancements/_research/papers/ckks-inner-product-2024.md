---
source_type: paper
date_published: 2024-03-15
hype_score: 1
---

# CKKS Inner Product / Taiyi Accelerator + P2P-CKKS

## Source reference

- Taiyi paper: https://arxiv.org/abs/2403.10188
- P2P-CKKS: https://link.springer.com/article/10.1186/s43067-025-00276-z
- ScienceDirect survey: https://www.sciencedirect.com/org/science/article/pii/S1546221825007702

## Claim summary

CKKS supports approximate-arithmetic FHE on floating-point values — including inner products, exactly the primitive cosine similarity needs. Inner-product is identified as the **bottleneck operation** in modern FHE applications (replacing NTT). Recent work like Taiyi (2024) builds dedicated hardware accelerators; P2P-CKKS pads vectors to power-of-two for faster computation.

## Relevance to our project

A2 (Local-first / privacy): Background for the encrypted-vector recommendation. CKKS is the FHE scheme of choice for ML / inner-product workloads (vs TFHE which is better for boolean / integer arithmetic). The project's vault Work file should be evaluated for whether CKKS or TFHE-rs is the better fit; the answer leans CKKS for cosine-similarity, TFHE for tag matching.

A1: Pure-Rust CKKS implementations exist (e.g., `concrete` from Zama). The audience reads CKKS on the same axis as TFHE.

## Specific takeaways

- For 512-d cosine similarity over thousands of images, CKKS with batched packing is the practical FHE path.
- Plaintext slots in CKKS hold up to ~16k floats — naturally amortises across multiple images in one ciphertext operation.
- The honest 4-5 OOM slowdown vs plaintext stands.

## Hype indicators

Mild — multiple substantive sources, no single-vendor framing.
