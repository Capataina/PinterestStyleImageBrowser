---
source_type: paper
date_published: 2024-10
hype_score: 0
---

# Private ANN Search — Pacmann (ICLR 2025) and Panther (CCS 2025)

## Source reference

- Pacmann (eprint): https://eprint.iacr.org/2024/1600
- Pacmann (ICLR 2025): https://openreview.net/forum?id=yQcFniousM
- Panther (eprint): https://eprint.iacr.org/2024/1774
- Panther (CCS 2025): https://dl.acm.org/doi/10.1145/3719027.3765190

## Claim summary

Two 2024 advances in private ANN search:
- **Pacmann** — combines graph-based ANN with PIR-compatible subgraph retrieval. Up to **2.5× better search accuracy** than prior private-ANN schemes.
- **Panther** — co-designs PIR + secret sharing + garbled circuits + HE. **9.3× lower communication cost** than prior methods.

## Relevance to our project

A2 + A4: Crucial substantive backing for the encrypted-vector recommendation. These papers establish that private ANN is not stuck at "infeasibly slow" — the field is making real algorithmic progress. The user's vault `Encrypted Vector Search.md` MVP can cite Pacmann / Panther as the algorithmic bound on what's tractable.

## Specific takeaways

- Both papers build on standard FHE primitives — TFHE-rs / OpenFHE / Microsoft SEAL all instantiate the building blocks.
- The Pacmann/Panther style is *server-side*; the project is local-only. The relevant adaptation is "two-process FHE" where one process holds plaintext and the other holds encrypted index.
- For a portfolio-quality artefact, citing top-tier venues (ICLR, CCS) is high-signal.

## Hype indicators

None — peer-reviewed top-tier venues.
