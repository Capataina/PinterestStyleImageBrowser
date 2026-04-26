---
source_type: paper
date_published: 2025-07
hype_score: 1
---

# ONNX Runtime Quantization (Static / Dynamic INT8)

## Source reference

- ONNX Runtime docs: https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html
- Selective Quantization Tuning paper (2025): https://arxiv.org/html/2507.12196v1
- Microsoft Open Source Blog: https://opensource.microsoft.com/blog/2022/05/02/optimizing-and-deploying-transformer-int8-inference-with-onnx-runtime-tensorrt-on-nvidia-gpus/

## Claim summary

ONNX Runtime supports static INT8 quantization (offline calibration on representative dataset) and dynamic quantization (per-forward calculation). Typical 2-4× speedup with minimal accuracy loss. CPU supports U8U8/U8S8/S8S8; GPU supports S8S8 only.

## Relevance to our project

A1 (Rust+ML): The project ships float32 CLIP. INT8 quantization of the image encoder would 2-4× the cold-start encode pass on CPU — the dominant cost on first-launch. The existing `ort` crate path supports loading INT8 ONNX models without any code change beyond the model file.

A4: For the embedding-quality audit, INT8 vs FP32 cosine-similarity preservation is a measurable artefact: encode the same images both ways, compare cosine of (FP32, INT8) embeddings. Industry reports cosine ≥ 0.99 for static-quantised CLIP.

## Specific takeaways

- Static quantization needs ~100-1000 calibration images representative of the domain.
- The text encoder is also quantisable; calibration would use a held-out query set.
- Plug-and-play: produces a new `model_image_int8.onnx` file, swappable behind a config flag.

## Hype indicators

Mild — Microsoft markets ONNX Runtime aggressively but the underlying quantization techniques are well-established (TensorFlow Lite, PyTorch all do equivalent).
