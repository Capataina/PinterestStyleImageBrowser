---
source_type: paper
date_published: 2023-03-27
hype_score: 1
---

# EVA-CLIP: Improved Training Techniques for CLIP at Scale

## Source reference

- arXiv: https://arxiv.org/abs/2303.15389
- Authors: Quan Sun, Yuxin Fang, Ledell Wu, Xinlong Wang, Yue Cao
- Code: https://github.com/baaivision/EVA/tree/master/EVA-CLIP

## Claim summary

EVA-CLIP improves CLIP training via better representation learning, optimisation, and augmentation. Largest 5B-param EVA-02-CLIP-E/14+ reaches 82.0 ImageNet zero-shot at 9B seen samples; 430M EVA-02-CLIP-L/14+ reaches 80.4 at 6B samples. Followed by EVA-CLIP-18B (Feb 2024).

## Relevance to our project

A4: Another alternative encoder for the embedding-quality audit. EVA-CLIP-L weights are practical for CPU desktop inference; the 18B variant is reference-only.

A1: Reinforces the broader "CLIP encoder family is now the SigLIP / MobileCLIP / EVA-CLIP / DFN-CLIP era; OpenAI CLIP is legacy" current.

## Specific takeaways

- EVA-CLIP weights are HuggingFace-hosted and convertible to ONNX via the standard pipeline.
- EVA-CLIP-L is a sensible mid-size option between the project's current ViT-B/32 (small) and SigLIP-Large (heavier).
- License: MIT.

## Hype indicators

None — primary research, named lab.
