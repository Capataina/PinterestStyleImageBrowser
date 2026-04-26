---
source_type: shipped-project
date_published: 2024-08
hype_score: 0
---

# Apple swift-homomorphic-encryption (BFV)

## Source reference

- GitHub: https://github.com/apple/swift-homomorphic-encryption
- Swift.org announcement: https://www.swift.org/blog/announcing-swift-homomorphic-encryption/

## Claim summary

Apple-open-sourced (Apache 2.0) Swift implementation of BFV homomorphic encryption + PIR primitives. **Used in production in iOS 18 Live Caller ID Lookup.** Includes Hummingbird HTTP framework wiring for cross-language interop, Swift Crypto for low-level crypto primitives, dedicated benchmarking suite.

## Relevance to our project

A2 (privacy-eng): Strongest possible production-shipped FHE example. Apache 2.0 licensed, so the patterns + protocol formats are fully studyable for a Rust port. The user's vault `Encrypted Vector Search.md` Open Item lists "Integration with `swift-homomorphic-encryption` for cross-language interoperability with Apple's BFV implementation" — this is the source.

## Specific takeaways

- BFV is the practical FHE choice for *integer-arithmetic* PIR / lookup; CKKS is the choice for *floating-point* (cosine inner product). The encrypted-vector recommendation may need both.
- Apple's serialised BFV ciphertexts are protobuf-defined; cross-language reader/writer is straightforward.
- Production deployment validates FHE-PIR is shippable at consumer scale.

## Hype indicators

None — Apple engineering blog + open code + production deployment.
