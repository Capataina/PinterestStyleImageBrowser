---
source_type: paper
date_published: 1998-08
hype_score: 0
---

# The Use of MMR, Diversity-Based Reranking for Reordering Documents and Producing Summaries

## Source reference

- ACM SIGIR 1998: https://dl.acm.org/doi/10.1145/290941.291025
- PDF: https://www.cs.cmu.edu/~jgc/publication/The_Use_MMR_Diversity_Based_LTMIR_1998.pdf
- Authors: Jaime Carbonell, Jade Goldstein (CMU)

## Claim summary

Maximal Marginal Relevance (MMR) — a re-ranking criterion that balances relevance to a query against redundancy with already-selected items. `MMR = argmax_d [λ Sim(d, q) - (1-λ) max_{d' in S} Sim(d, d')]`. Iteratively builds a result set that is both relevant and diverse.

## Relevance to our project

A4 + A1: The project's `get_tiered_similar_images` is a custom diversity-aware retriever. The vault note (`Suggestions.md` rec 5) flags it as "load-bearing product design" — but it has not been compared against MMR or k-DPP, the canonical diversity-aware retrieval methods.

A direct additive recommendation: implement an MMR mode behind the existing `CosineIndex` interface as a fourth retrieval mode (alongside sampled / sorted / tiered). The project then has a comparison story: "ours vs MMR vs k-DPP", which is exactly the comparison this audience reads.

## Specific takeaways

- MMR is parameterless except for `λ` (relevance vs diversity weight). λ=0.7 is a common default.
- MMR with cosine-similarity inputs is trivially implementable on top of the existing `CosineIndex.cosine_similarity` primitive.
- The 7-tier sampler is an *implicit* diversification heuristic; MMR is the *explicit* one. Both have a place; the comparison surfaces the principled framing.

## Hype indicators

None — foundational paper with 1000+ citations.
