# dinov2-encoder

*Maturity: working*

## Scope / Purpose

Image-only encoder using Meta's self-supervised DINOv2-Base (CVPR 2023). Produces 768-dimensional L2-normalised `f32` embeddings optimised for image→image retrieval — particularly fine-grained "same character", "same pose", "same art style" queries where DINOv2 dominates CLIP-style models (the research agent's investigation cited a ~5× advantage on fine-grained 10k-class species benchmarks).

Has no text branch by design — DINOv2 was trained without text alignment. Used in the project as the recommended "View Similar" (image-clicked) encoder; text→image queries continue to use CLIP or SigLIP-2.

## Boundaries / Ownership

- **Owns:** image preprocessing (aspect-preserving bicubic resize on shortest edge to 256, then center-crop to 224, ImageNet normalization), ONNX session lifecycle (CPU only — DINOv2 isn't CoreML-tested), single-image inference, CLS-token extraction from `last_hidden_state[:, 0, :]`, L2-normalize at output, the encoder URL + filename + ID constants.
- **Does not own:** writing embeddings to disk (delegates to `db.upsert_embedding(..., encoder_id="dinov2_base", ...)`), text-side encoding (none — image-only), the model file on disk (delegates to `paths::models_dir()` + `DINOV2_IMAGE_MODEL_FILENAME`).
- **Public API:** `Dinov2ImageEncoder::new(model_path)` plus the trait impl. Constants: `DINOV2_IMAGE_MODEL_URL`, `DINOV2_IMAGE_MODEL_FILENAME`, `DINOV2_ENCODER_ID`. Implements the `ImageEncoder` trait so the indexing pipeline dispatches via `Box<dyn ImageEncoder>`.

## Current Implemented Reality

### Pipeline per image — canonical DINOv2 BitImageProcessor preprocessing

```text
ImageReader::open → with_guessed_format → decode → to_rgb8
    │
    ▼
Aspect-preserving resize on shortest edge to 256 (CatmullRom — image-rs's bicubic-family
    filter, closest match to PIL's BICUBIC). NOT resize_exact.
    │
    ▼
Center-crop to 224×224. (Larger initial resize than CLIP's shortest-edge-224 — more
    margin around the subject before cropping.)
    │
    ▼
Split RGB into 3 contiguous slices (CHW layout)
    │
    ▼
Normalise per-channel: x = (x/255 - mean[c]) / std[c]
    where mean = [0.485, 0.456, 0.406]   ← ImageNet-stats (DINOv2 was trained on ImageNet)
          std  = [0.229, 0.224, 0.225]      Distinct from CLIP-stats and SigLIP-stats.
    │
    ▼
Reshape to ndarray::Array4 with shape (1, 3, 224, 224)
    │
    ▼
ort::Tensor::from_array((shape, raw_vec))
    │
    ▼
Session::run with ONE input:
    pixel_values  ← image tensor [1, 3, 224, 224]
    │
    ▼
Output: last_hidden_state, shape [1, 257, 768]
    where 257 = 1 CLS token + 256 patch tokens (224/14)²
    │
    ▼
Extract CLS token: data[..768] (first 768 floats = first row of the sequence dimension)
    │
    ▼
L2-normalize via super::encoder_text::pooling::normalize
    └─► return Vec<f32> length 768
```

### Why the ImageNet stats matter

DINOv2 was trained on ImageNet with the standard ImageNet mean/std. Feeding it CLIP-stat-normalised inputs (or SigLIP's [-1, 1] inputs) silently shifts the input distribution away from training-time, which degrades embedding quality without any error signal. Each encoder must be fed the distribution it was trained on — that's why `encoder.rs`, `encoder_dinov2.rs`, and `encoder_siglip2.rs` each carry their own preprocessing pipeline rather than sharing a generic one.

### CLS-token extraction (no pooler_output)

Xenova's DINOv2-Base ONNX export terminates at the final LayerNorm — it does NOT export a `pooler_output`. The encoder slices the CLS token from `last_hidden_state` directly:

```rust
if let Some(t) = outputs.get("last_hidden_state") {
    let (shape, view) = t.try_extract_tensor::<f32>()?;
    let data = view.to_vec();
    let hidden = if shape.len() == 3 { shape[2] as usize } else { 768 };
    data[..hidden].to_vec()           // CLS = first row
}
```

The encoder also tries `pooler_output` first as a defensive fallback in case a future export adds it, but in practice the `last_hidden_state` branch is what fires.

### Storage namespace

The encoder ID is **`dinov2_base`**, not `dinov2_small`. Old 384-d embeddings produced by the previous `dinov2_small` encoder are abandoned in the embeddings table (encoder_id `dinov2_small`) — they coexist harmlessly until the migration cleans them up. The `migrate_embedding_pipeline_version` (DB version 2) deletes those rows on first launch under the new code, freeing disk and removing them from the pipeline-stats UI.

## Key Interfaces / Data Flow

```
indexing.rs::run_trait_encoder("dinov2_base", make_encoder=Dinov2ImageEncoder::new):
    dinov2_path = paths::models_dir().join(encoder_dinov2::DINOV2_IMAGE_MODEL_FILENAME)
    if dinov2_path.exists():
        needs = db.get_images_without_embedding_for("dinov2_base")
        for chunk in needs.chunks(32):
            embeddings = encoder.encode_batch(chunk_paths)   // trait default: per-image loop
            for ((id, _path), embedding) in chunk.zip(embeddings):
                db.upsert_embedding(id, "dinov2_base", embedding)
            emit Phase::Encode(processed, total)
        emit "encoder_run_summary" diagnostic
        emit "preprocessing_sample" diagnostic on first batch
    else:
        warn "DINOv2 image model missing at {dinov2_path}; skipping"
```

## Implemented Outputs / Artifacts

- `<app_data_dir>/models/dinov2_base_image.onnx` (~347 MB) loaded at construction.
- 768-d L2-normalised `f32` embedding per image.
- One storage destination: `embeddings(image_id, encoder_id="dinov2_base", embedding)` row.
- Encoder is recreated per indexing-pipeline run.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Center-crop drops edge content (same as CLIP) | Tall/wide images with meaningful periphery | Embeddings reflect only the central 224×224 window. See `notes/preprocessing-spatial-coverage.md`. |
| Inference is ~4× slower than the previous DINOv2-Small | Library size that was previously fast under -Small | Encoding pass takes ~4× longer. M-series CPU absorbs it for 1500–10k images; larger libraries may want to be selective about which encoders to enable. |
| Trait-default `encode_batch` falls back to per-image inference | Large batches | DINOv2's ONNX export doesn't natively batch in this Rust path; the trait default loops one-by-one. ~5–10% potential speedup if the ONNX session were called with a batched tensor instead. |
| 768-d cache uses ~2× the memory of 384-d | Large libraries | Cosine cache size doubles. Negligible for 10k images (still under 30 MB). |
| EP fallback is silent | Future CUDA EP attempt failing | Runs on CPU at 10× slower throughput with no UI signal. The encoder currently doesn't try CUDA — straight CPU session — so this is forward-looking. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Native batched inference** — feed `[batch_size, 3, 224, 224]` tensors instead of per-image; would need a batched preprocessor first.
- **DINOv2-Large (1024-d)** as an opt-in for users who want even higher fidelity at ~3× the disk + inference cost.
- **DINOv2 native 518×518 resolution** — the model's pretraining resolution. Xenova's export downsizes to 224 (matching ImageNet eval), but a re-exported 518-input variant would let the model see more detail at the cost of ~5× compute.
- **Smart per-query encoder routing** — long-term, View-Similar queries route here; semantic queries route to CLIP/SigLIP-2. Open in `notes/preprocessing-spatial-coverage.md`.

## Durable Notes / Discarded Approaches

- **DINOv2-Small (384-d) was a quality compromise.** The Small variant has limited capacity for fine-grained discrimination on a homogeneous corpus (e.g., anime art of one character). Base doubles hidden width and quadruples parameter count; the user-visible discrimination delta was the deciding factor. Cost: ~4× inference, ~4× disk.
- **The previous `resize_exact(224, 224)` + Lanczos3 + ImageNet-stats path was wrong on the geometry.** DINOv2 was trained with shortest-edge-256 + center-crop-224; using `resize_exact` squashed non-square images. The current pipeline matches the canonical `BitImageProcessor` settings from `facebook/dinov2-base/preprocessor_config.json` (verified 2026-04-26 by parallel research agent).
- **No `pooler_output` is exported.** Confirmed by reading the ONNX graph header in the verification pass — `last_hidden_state` is the only output. The CLS-token slice is the canonical way to extract the image representation per the official DINOv2 README.
- **No CoreML / CUDA attempt.** The encoder takes the straightforward CPU path. CoreML rejection is documented for CLIP; DINOv2 hasn't been validated. Adding EPs is straightforward future work but requires testing.
- **Encoder ID change `dinov2_small` → `dinov2_base` was deliberate** so the dim mismatch (384 → 768) is automatically handled by the per-encoder embeddings table (no rows for `dinov2_base` exist on the first run after the swap, so the indexing pipeline encodes everything fresh). The old `dinov2_small` rows are wiped by `migrate_embedding_pipeline_version` (DB version 2).

## Obsolete / No Longer Relevant

The DINOv2-Small (384-d) variant and its `model_dinov2_image.onnx` filename. The `resize_exact(224, 224)` + Lanczos3 preprocessing.
