---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# Hugging Face Optimum — ONNX Export + Quantization

## Source reference

- Optimum GitHub: https://github.com/huggingface/optimum
- Optimum-ONNX: https://github.com/huggingface/optimum-onnx
- Quantization docs: https://huggingface.co/docs/optimum-onnx/en/onnxruntime/usage_guides/quantization
- CLIP quantization discussion: https://discuss.huggingface.co/t/how-to-quantize-and-run-inference-for-clip-using-optimum/89631

## Claim summary

HF's framework for ONNX export + ONNX Runtime quantization. CLIP is supported as a ready-made config. Quantization is via `ORTConfig` + `ORTQuantizer`. Outputs an `model_quantized.onnx` file usable directly by ONNX Runtime (and therefore `ort` in Rust).

## Relevance to our project

A1 + A4: For the embedding-quality audit + per-encoder benchmark recommendation, Optimum provides the standard pipeline to export + INT8-quantize CLIP / SigLIP / DINOv2 / MobileCLIP variants. Consistent across encoders, well-documented.

## Specific takeaways

- The Python script + a small calibration dataset → produces ONNX file → drops into the project's existing `ort` pipeline.
- INT8 quantization gives 2-4× CPU speedup with cosine ≥ 0.99 vs FP32.
- Static quantization with calibration is preferred for image encoders; dynamic for text encoders.

## Hype indicators

None — official HF infrastructure.
