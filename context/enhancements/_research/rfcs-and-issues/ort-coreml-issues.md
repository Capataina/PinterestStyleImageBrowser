---
source_type: github-rfc
date_published: 2025-12
hype_score: 0
---

# ONNX Runtime CoreML EP — Limited Transformer Op Support

## Source reference

- microsoft/onnxruntime#19887: CoreML 25% node coverage on transformer
- microsoft/onnxruntime#17654: CoreML not used to fullest, custom transformer
- microsoft/onnxruntime#16934: CoreML supports 4/9 nodes
- microsoft/onnxruntime#21227: SIGSEGV on CoreML with dynamic batch
- pykeio/ort#475: cuda EP unavailable when other EPs enabled

## Claim summary

CoreML execution provider has known gaps for transformer ops. Many community reports of 25-50% node coverage — the rest fall back to CPU. Dynamic batch dimensions sometimes SIGSEGV on CoreML. The project's own commits `90a3842` and `2775c9f` document the same issue (CoreML disabled for both encoders due to "transformer ops poorly supported" + "runtime inference errors").

## Relevance to our project

A1: The project's CoreML disable is *the correct workaround* per the upstream issue body. A short writeup of the diagnosis + workaround is portfolio-shaped — exactly the kind of "understands the stack deeply enough to debug it" signal A1 reads.

The recommendation downstream: an explicit "ONNX EP Compatibility Matrix" file (`docs/EP_COMPAT.md` or similar) that documents which EPs work for which model + the workaround pattern. This is a low-cost, high-signal artefact.

## Specific takeaways

- CoreML EP shines on classical CNN graphs (Yolo, ResNet) and struggles with transformer attention.
- Workaround patterns: disable CoreML in EP list, force CPU EP for the failing model.
- This pattern is the "documenting hardware-aware fallbacks" artefact A1 specifically rewards.

## Hype indicators

None — issue tracker, real bug reports.
