---
audience: Local-first / privacy-engineering community
secondary_audiences: ML-infra and retrieval / embedding-systems researchers
coupling_grade: commitment-grade
implementation_cost: large (8-12 weeks)
status: draft
---

# Encrypted vector search MVP — TFHE-rs encrypted CosineIndex (additive opt-in mode)

## What the addition is

A second `VectorIndex` implementation alongside the brute-force and HNSW: `EncryptedCosineIndex`, built on TFHE-rs (Zama). Stores image embeddings as TFHE ciphertexts in a parallel SQLite column (`embedding_ciphertext` BLOB). Supports encrypted top-K linear scan only — encrypted dot-product on each candidate, compare encrypted scalars, return encrypted ranking. The 7-tier sampler and other diversity-aware modes are explicitly **named-not-supported** for the encrypted path (the FHE envelope doesn't fit them in real time).

Honest perf framing: 4-5 orders of magnitude slowdown vs plaintext per-query. Use case: sensitive image collections where the user accepts the slowdown for the privacy guarantee.

The user's vault `Encrypted Vector Search.md` (LifeOS Projects/Image Browser/Work/) already drafts this; this rec formalises the engineering plan.

## Audience targeted

**Primary: A2 Local-first / privacy-engineering community** — `audience.md` Audience 2 signal-function:
- "Crypto primitives in shipped code: TFHE-rs / BFV / SEAL bindings actually called, encrypted ciphertext-shaped values flowing through the index, named library + version" ✓
- "Threat-model clarity: Adversary named, assumption table, bound on what the encryption protects vs leaks" ✓
- "Honest perf framing: Documented FHE slowdown (orders of magnitude), tractable-envelope spec, named-not-supported retrieval modes" ✓
- "Comparable artefacts: Cite Apple Wally, Microsoft SEAL ports, Zama Concrete examples" ✓

This rec checks every box for A2.

**Secondary: A4** — Pacmann/Panther are top-tier-venue 2024 papers; the project's MVP cites them as the algorithmic envelope.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/projects/tfhe-rs-zama.md` | Pure-Rust TFHE library, $73M-funded sponsor, weekly releases, AVX-512 acceleration. The chosen FHE library. |
| 2 | `_research/papers/wally-apple-2024.md` | Apple's PNNS shipped in production iOS 18 — strongest possible durability signal for the FHE-on-vector-search direction. |
| 3 | `_research/projects/swift-homomorphic-encryption.md` | Apple's open-sourced BFV + PIR primitives; cross-language interop reference. |
| 4 | `_research/papers/ckks-inner-product-2024.md` | CKKS supports floating-point inner products natively; technical foundation for the cosine path. Recent academic work on CKKS bottleneck analysis. |
| 5 | `_research/papers/pacmann-panther-2024.md` | ICLR 2025 (Pacmann), CCS 2025 (Panther). The state-of-the-art private ANN search papers; algorithmic envelope reference. |
| 6 | `_research/projects/zama-concrete-ml.md` | Zama's higher-level FHE-ML toolkit; demonstrates the ecosystem maturity. |
| 7 | `_research/firm-hiring/apple-pcc.md` | Apple's PCC team specifically hires for this exact engineering work. The audience-fit is direct. |
| 8 | `_research/projects/silentkeys-tauri-ort.md` | Reference for "Tauri+Rust+ORT app with stronger privacy stance"; SilentKeys' "audio never leaves device" is structurally analogous to Image Browser's "embeddings never leave device when encrypted". |
| 9 | `_research/notes` (vault) — `Work/Encrypted Vector Search.md` | The user has *already drafted* this. The rec formalises the existing intent. |
| 10 | `_research/projects/instant-distance.md` | Even with HNSW (Rec-2), the encrypted index uses linear scan — the FHE envelope. Documented honestly. |
| 11 | `_research/projects/parking-lot-mutex.md` | Cross-coupling: encrypted index lock contention is real; non-poisoning Mutex is part of the production-grade story. |
| 12 | `_research/funding/vector-db-funding-2024.md` | Vector-DB is a funded category; encrypted vector search is the next frontier within it. |
| 13 | `_research/projects/lancedb.md` | Cross-coverage: even mainstream embedded vector DBs are starting to discuss encrypted vector storage. The category exists. |

## Coupling-grade classification

**Commitment-grade.** Adopting FHE for a real query path is structurally non-trivial — encryption setup, key management, ciphertext schema migration, encrypted-domain primitives. Removing it after adoption is a multi-day refactor.

### Durability case

Per Hard Constraint 3 / `references/research-method.md` §commitment-grade:

| Required signal | Evidence |
|---|---|
| Multiple substantive papers | Wally (Apple), Pacmann (ICLR 2025), Panther (CCS 2025), CKKS bottleneck paper (Taiyi 2024) |
| Real frontier signal (named research labs) | Apple ML Research, Zama, MIT, CMU, NYU |
| Funded companies shipping | Zama ($73M+), Inpher, Optalysys; Apple shipping in production |
| Audience signal-function alignment | A2 signal-function explicitly names FHE / private-NN-search as the audience-fit pattern |

The commitment is justified.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, organised around a `CosineIndex` brute-force similarity engine over SQLite-stored f32 BLOBs.** Rec-1 introduced the `VectorIndex` trait. Rec-2 added an HNSW impl. Rec-7 adds an *encrypted* third impl that lives strictly behind a feature flag. The plaintext path stays the default; nothing in the existing code path changes. The encrypted path becomes available to users who want it.

```
   Three index variants behind one trait
   ┌─────────────────────────────────────────────────┐
   │   trait VectorIndex                              │
   │       │                                          │
   │       ├ BruteForceCosineIndex   (default)       │
   │       ├ HNSWVectorIndex         (opt-in scale)  │
   │       └ EncryptedCosineIndex    (opt-in privacy)│
   └─────────────────────────────────────────────────┘

   Schema:
   ┌─────────────────────────────────────────────────┐
   │ images:                                          │
   │   embedding         BLOB  -- existing FP32      │
   │   embedding_ciphertext BLOB  -- new TFHE-rs     │
   │                              ciphertext         │
   └─────────────────────────────────────────────────┘
```

**Tractable retrieval modes with FHE:**
- ✅ Single-pair encrypted similarity (one image vs one image) — viable.
- ✅ Encrypted top-K via linear scan (one query vs N images) — viable, slow.
- ❌ 7-tier diversity sampler over thousands of images in real time — *not* tractable. Named-not-supported in the EncryptedCosineIndex `search()` impl with an explicit error.
- ❌ MMR / k-DPP (Rec-5) require pairwise candidate-vs-candidate comparisons; not tractable at FHE scale. Named-not-supported.

**Key management:** the project owns the FHE keys (it's the user's machine; the user is the only party). The "encryption" protects the *embeddings on disk* from any process that doesn't hold the key, not from the project itself. This is a defence-in-depth posture for sensitive collections (medical / legal / journalistic), not a zero-trust posture.

**Reference-app surface:** a "Privacy Mode" toggle in the Settings drawer (which already exists per commit `a66d1f7`). When on, the encrypted index serves searches; a UI badge indicates "ciphertext only" lifetime of the query.

## Anti-thesis

This recommendation would NOT improve the project if:

- The user decides Image Browser is solely a personal-collection tool with no sensitive-data use case. Then the FHE work is overkill.
- The plaintext path is already considered private enough (the data never leaves the user's machine in either case).
- The 4-5 OOM slowdown makes the encrypted mode unusable for any real workflow. The benchmark from Rec-2 should be extended here to show *exact* numbers; if the user finds them unbearable, the rec stands as portfolio-evidence-only rather than user-facing feature.
- TFHE-rs's licensing (BSD 3-Clause Clear, commercial-patent-licence required for commercial use) is incompatible with the user's project licensing intent.

## Implementation cost

**Large: 8-12 weeks** (significantly larger than other recs; reflects the FHE on-ramp).

Milestones:
1. **Spike phase (1-2 weeks).** Isolated TFHE-rs prototype: encrypt a 512-d embedding, compute encrypted dot-product against a plaintext query, decrypt result. Measure latency. Get a feel for the ciphertext expansion factor (50-500×). ~10 days.
2. **Schema migration + `EncryptedCosineIndex` skeleton (1-2 weeks).** Add `embedding_ciphertext` column, key-generation flow, the `VectorIndex` trait impl with linear-scan top-K. ~10 days.
3. **Honest benchmark (1 week).** Per-query latency on the bundled `test_images/` corpus. Per-image storage cost. Document honestly — the rec stands or falls on the honesty of these numbers. ~5 days.
4. **Reference UI + key-management UX (1-2 weeks).** Settings drawer toggle, key-derivation from user-chosen passphrase, recovery flow (passphrase loss = encrypted index loss). ~10 days.
5. **Documentation + threat model (1 week).** A new `context/systems/encrypted-vector-search.md` documenting the threat model, what FHE protects, what it leaks, what's not supported. The honest writeup is the portfolio artefact. ~5 days.
6. **Optional ambitious extensions (skipped unless appetite remains):** structured tree indexes, swift-homomorphic-encryption interop, differential-privacy noise calibration. Per the user's vault, these are explicitly out of MVP scope.

**Sequencing note:** The user's vault explicitly gates this work behind "Image Browser v1 polish (folder picker UI, multi-folder support, search-bar UX) shipped first". Per recent commits (`47435f9` folder picker, `0908550` multi-folder, `a66d1f7` settings drawer, `56990b7` AND/OR tag filters), v1 is largely shipped. The gate is mostly satisfied.

Required reading before starting: re-read the user's vault `Work/Encrypted Vector Search.md` for the existing intent + the Apple Wally / Pacmann / Panther papers for the algorithmic envelope.
