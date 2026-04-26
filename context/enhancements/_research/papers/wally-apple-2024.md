---
source_type: paper
date_published: 2024-08
hype_score: 1
---

# Scalable Private Search with Wally

## Source reference

- Apple ML Research: https://machinelearning.apple.com/research/wally-search
- Live Caller ID example: https://github.com/apple/live-caller-id-lookup-example
- Swift Homomorphic Encryption: https://www.swift.org/blog/announcing-swift-homomorphic-encryption/

## Claim summary

Apple's production deployment of FHE-based **private nearest-neighbour search** in iOS 18. The client encrypts a vector embedding, sends it to the server, the server performs encrypted-domain nearest-neighbour search, returns encrypted results that the device decrypts. Powers Live Caller ID Lookup and the Visual Look Up backend.

## Relevance to our project

A2 (Local-first / privacy): The strongest possible durability signal for FHE-on-vector-search — Apple has shipped it in production at consumer scale. The user's existing vault Work file (`Encrypted Vector Search.md`) explicitly cites Wally as the validation point for the BFV path.

A1 + A4: The Wally architecture is server-side; the project is client-only. But the *encrypted-similarity-over-CLIP-embeddings* primitive is the same. A local-only TFHE-rs adaptation (where "the server" is actually a separate process or volume) is a defensible additive direction.

## Specific takeaways

- Apple's swift-homomorphic-encryption library (BFV-based) is open-source, implementing the same primitives in Swift.
- The exact PNNS protocol uses a clustering preprocessing step on the server's data before FHE; this is what TFHE-rs would need to mirror to be practical.
- Wally validates that the FHE slowdown envelope is *workable* for the right problem shape (small batched lookups, not real-time ranking).

## Hype indicators

None — primary research blog from a named lab + open code in production deployment.
