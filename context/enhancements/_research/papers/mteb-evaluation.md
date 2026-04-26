---
source_type: paper
date_published: 2024-09
hype_score: 1
---

# MTEB / MTEB v2 — Massive Text Embedding Benchmark

## Source reference

- GitHub: https://github.com/embeddings-benchmark/mteb
- Hugging Face leaderboard: https://huggingface.co/spaces/mteb/leaderboard
- v2 announcement: https://huggingface.co/blog/isaacchung/mteb-v2

## Claim summary

Standardised benchmark for text embedding models — retrieval (nDCG@10), classification, clustering, reranking, semantic similarity. v2 (2024) added multimodal-model support including CLIP-style retrieval. The canonical leaderboard for embedding-model evaluation.

## Relevance to our project

A4: For the embedding-quality audit recommendation, MTEB-v2 multimodal tracks provide a public benchmark to position the project's encoder choice. Even a small subset comparison ("we ran MTEB-Visual-IR-en on our 749-image dataset across CLIP-B/32 / SigLIP-B / MobileCLIP and got these numbers") is a strong portfolio artefact.

## Specific takeaways

- nDCG@10 is the canonical retrieval metric on MTEB.
- MTEB v2 supports image-text retrieval natively.
- The leaderboard-shaped artefact ("our scores are X for these 4 encoders against this benchmark slice") is exactly the kind of citable output A4 reads.

## Hype indicators

None — neutral benchmark suite.
