---
source_type: paper
date_published: 2024-04-11
hype_score: 1
---

# MobileCLIP: Fast Image-Text Models through Multi-Modal Reinforced Training

## Source reference

- Apple Machine Learning Research: https://machinelearning.apple.com/research/mobileclip
- Code: https://github.com/apple/ml-mobileclip
- Venues: CVPR 2024 (MobileCLIP), TMLR 2025 (MobileCLIP2)
- Authors: Pavan Kumar Anasosalu Vasu et al. (Apple)

## Claim summary

Family of efficient image-text models optimised for **3-15ms latency** at **50-150M parameters**. MobileCLIP-S2 is "2.3× faster while more accurate" than ViT-B/16 CLIP. Uses MobileOne-style vision towers + multi-modal reinforced training (knowledge transfer from a captioning model + ensemble of strong CLIP encoders) for 10-1000× learning efficiency vs non-reinforced CLIP training.

## Relevance to our project

A1 + A2 + A4: The project ships ViT-B/32 CLIP via ONNX. Embedding pass on CPU is the dominant cold-start cost (the project's own encoder.rs test heuristic puts it at 0.5-1.5 s per image). MobileCLIP variants would drop that 5-10× while maintaining or improving recall@k. For the local-first audience (A2), the smaller model size also means smaller bundles and lower RAM peak — both of which matter when the app ships to users on consumer Macs / older Windows laptops.

## Specific takeaways

- Apple ships MobileCLIP weights in Core ML AND ONNX formats — drop-in for the existing `ort` pipeline.
- MobileCLIP2 (Aug 2025) further improves on MobileCLIP1.
- Embedding dim varies by variant (S0/S1/S2/B variants); the 512-d storage schema can be preserved by selecting the matching variant or by re-projection.
- Apple has shipped MobileCLIP in production (Visual Look Up). This is a strong durability signal.

## Hype indicators

None — Apple research with code, weights, and shipping product.
