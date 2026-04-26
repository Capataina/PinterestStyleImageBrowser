# clip-image-encoder

*Maturity: comprehensive*

## Scope / Purpose

Loads OpenAI CLIP ViT-B/32's separate `vision_model.onnx` and produces 512-dimensional L2-normalised `f32` embeddings for image files. Runs CPU-only on macOS (CoreML's runtime inference fails on this graph) and tries CUDA on non-macOS, both with CPU fallback. Driven by the indexing pipeline's encode phase to populate `images.embedding` BLOB + the per-encoder `embeddings(image_id, encoder_id="clip_vit_b_32", embedding)` row for every image lacking one.

## Boundaries / Ownership

- **Owns:** image preprocessing (aspect-preserving bicubic resize + center-crop + CLIP-native normalization), ONNX session lifecycle with EP fallback, single + batched inference, L2-normalize at output.
- **Does not own:** writing embeddings to disk (delegates to `db.update_image_embedding` + `db.upsert_embedding`), CUDA / CoreML detection (relies on `ort`'s built-in fallback semantics), the model file on disk (delegates to `paths::models_dir()` + `model_download::CLIP_VISION_FILENAME`), text-side encoding (lives in `clip-text-encoder`).
- **Public API:** `ClipImageEncoder::new(model_path)`, `inspect_model`, `preprocess_image`, `batch_preprocess_image`, `encode`, `encode_batch`. Implements the `ImageEncoder` trait so the indexing pipeline can dispatch via `Box<dyn ImageEncoder>`.

## Current Implemented Reality

### Pipeline per image — canonical OpenAI CLIP preprocessing

```text
ImageReader::open → with_guessed_format → decode → to_rgb8
    │
    ▼
Aspect-preserving resize on shortest edge to 224 (CatmullRom — image-rs's bicubic-family
    filter, closest match to PIL's BICUBIC). NOT resize_exact (which would squash).
    │
    ▼
Center-crop to 224×224. Cuts away anything outside the central window — see
    notes/preprocessing-spatial-coverage.md for the implication on edge content.
    │
    ▼
Split RGB into 3 contiguous slices (R then G then B):
    [R0..R(224*224-1), G0..G(...), B0..B(...)]      # length 224*224*3
    │
    ▼
Normalise per-channel: x = (x/255 - mean[c]) / std[c]
    where mean = [0.48145466, 0.4578275, 0.40821073]   ← CLIP-native, from
                                                         Xenova preprocessor_config.json
          std  = [0.26862954, 0.26130258, 0.27577711]
    │
    ▼
Reshape to ndarray::Array4 with shape (1, 3, 224, 224)
    │
    ▼
ort::Tensor::from_array((shape, raw_vec))
    │
    ▼
Session::run with ONE input (separate vision_model.onnx — no dummy text inputs):
    pixel_values    ← image tensor
    │
    ▼
Output: image_embeds, shape [1, 512]
    │
    ▼
L2-normalize via super::encoder_text::pooling::normalize
    └─► return Vec<f32> length 512
```

### The "no dummy text inputs" change

Pre-2026-04-26 the encoder used Xenova's combined-graph CLIP export, which bundled image and text encoders in a single ONNX graph. Calling it for image-only inference required supplying dummy `input_ids: [[0]]` and `attention_mask: [[1]]` to satisfy the graph signature.

The current build uses the **separate** `vision_model.onnx` from the same Xenova repo. Inputs are reduced to just `pixel_values`, simplifying the call shape and removing the unused text branch from session memory. This was part of the same change that switched the text encoder from the multilingual distillation to OpenAI English (see `clip-text-encoder.md`) — both halves were swapped together to keep the embedding space consistent.

### Execution provider — CoreML disabled, CPU-only on macOS

```rust
#[cfg(target_os = "macos")]
fn build_session_with_accel(model_path: &Path) -> Result<Session, Box<dyn Error>> {
    // CoreML's GetCapability accepts ~54% of CLIP nodes (980 of 1827) and
    // session-create succeeds, but actual inference fails at runtime with
    // "Unable to compute the prediction using a neural network model
    // (error code: -1)". Documented in encoder.rs header comment block.
    Session::builder()?.commit_from_file(model_path)
}

#[cfg(not(target_os = "macos"))]
fn build_session_with_accel(model_path: &Path) -> Result<Session, Box<dyn Error>> {
    Session::builder()?
        .with_execution_providers([CUDAExecutionProvider::default().build()])?
        .commit_from_file(model_path)
}
```

CoreML was disabled mid-2026 after the runtime-failure pattern was confirmed across multiple ort releases. The encoder.rs file header carries the diagnosis: ort's CoreML EP partition decision is permissive at compile time but the resulting graph doesn't actually run. CPU on M-series is ~200–500 ms per image — acceptable for the project's library sizes (1500–10k images).

### Batch encoding

```rust
pub fn encode_batch(&mut self, paths: &[&Path]) -> Result<Vec<Vec<f32>>>
```

Pre-processes every image in `paths` serially (could be rayon-parallelised), then runs a single `Session::run` with a stacked tensor of shape `[batch_size, 3, 224, 224]`. Default batch size in the indexing pipeline is 32. Output is `[batch_size, 512]` which gets unstacked and L2-normalised per-row.

### Where it runs

The indexing pipeline calls `ClipImageEncoder::new(image_model_path)` once per pipeline run (so a re-spawn after `add_root` reloads the model — slightly wasteful, but the cost is bounded by how often pipelines re-spawn).

## Key Interfaces / Data Flow

```
indexing.rs::run_clip_encoder:
    image_model_path = paths::models_dir().join(model_download::CLIP_VISION_FILENAME)  // "clip_vision.onnx"
    if image_model_path.exists():
        needs_embed = db.get_images_without_embeddings()
        for chunk in needs_embed.chunks(32):
            embeddings = encoder.encode_batch(chunk_paths)
            for (image, embedding) in chunk.zip(embeddings):
                db.update_image_embedding(image.id, embedding.clone())   // legacy column
                db.upsert_embedding(image.id, "clip_vit_b_32", embedding) // per-encoder table
            emit Phase::Encode(processed, total)
        emit "encoder_run_summary" diagnostic (attempted/succeeded/failed/mean ms)
        emit "preprocessing_sample" diagnostic on first batch
    else:
        warn "{CLIP_VISION_FILENAME} missing; embeddings will be empty until next launch."
```

The encode phase is gated on the model file existing. If it doesn't (first launch + download in progress, user manually deleted), the pipeline skips encode entirely. Semantic + similarity search on a no-embedding library returns empty Vec.

## Implemented Outputs / Artifacts

- `<app_data_dir>/models/clip_vision.onnx` (~352 MB) loaded at construction.
- 512-d L2-normalised `f32` embedding per image.
- Two storage destinations per embedding:
  - Legacy `images.embedding` BLOB (kept for backward-compat with semantic_search reader)
  - New `embeddings(image_id, encoder_id="clip_vit_b_32", embedding)` row
- Encoder is recreated per indexing-pipeline run; not held in long-lived Tauri state.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Center-crop drops edge content | Tall/wide images with meaningful periphery | Embeddings reflect only the central 224×224 window. Splash arts / scenery / group photos with edge content are under-represented. See `notes/preprocessing-spatial-coverage.md`. |
| EP fallback is silent | CUDA init failure on non-macOS | Runs on CPU at 10× slower throughput with no UI signal. The user sees a slow encode phase but no error. |
| Encoder is re-instantiated per pipeline run | Frequent root mutations | Each `add_root` triggers a new pipeline → new `ClipImageEncoder::new` → re-load 352 MB model into ONNX session. Wasteful but bounded. |
| Sequential preprocessing within `encode_batch` | Large batches | Decoding 32 images sequentially before the ONNX batch is the bottleneck for some workloads. Could be rayon-parallelised. |
| Model file corruption surfaces only at session creation time | Disk corruption mid-download | `Session::builder().commit_from_file` errors → `ApiError::Encoder("ONNX session creation failed: ...")`. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Hold the encoder in Tauri state** to avoid re-loading on every pipeline re-spawn. Trade-off: ~352 MB resident memory always vs the load cost on rare re-spawns. Probably worth it now that the encoder is much smaller than before.
- **Rayon-parallel preprocessing** within `encode_batch` to overlap decode with the next batch's CPU work.
- **Smart per-query encoder routing** — long-term, route color/scenery queries to SigLIP-2 (no crop) and character/object queries to CLIP. Captured as an open concern in `notes/preprocessing-spatial-coverage.md`.
- **Int8 quantised image encoder** — would shrink the download by ~4× and speed up inference. Documented in `enhancements/recommendations/06-int8-quantisation-encoders.md`. Note: the user has explicitly rejected quantization on quality grounds for the current pipeline — revisit only if a use case justifies the trade-off.

## Durable Notes / Discarded Approaches

- **Combined-graph + dummy text inputs is gone.** The encoder now uses the separate `vision_model.onnx`. Old indexing data with dummy-text-input embeddings is invalidated by `migrate_embedding_pipeline_version` (DB version 2) — see `systems/database.md`. If a future ONNX export change requires re-invalidating, bump the version constant.
- **CoreML stays disabled even though "GetCapability" reports it can handle the graph.** The runtime inference failure pattern was reproducible across multiple ort releases. Re-enabling without verifying every op runs under inference is silent corruption waiting to happen.
- **Per-channel slice layout `[R..., G..., B...]` is intentional, not interleaved `[RGB, RGB, ...]`.** ONNX tensor convention is NCHW (channels first); interleaved would require a transpose at every encode call.
- **CatmullRom over Lanczos3 is the canonical bicubic match.** PIL's `BICUBIC` (resample=3) maps to a cubic family closer to CatmullRom than Lanczos3. Not bit-exact, but standard for ONNX deployments outside Python.
- **L2-normalize at output is required** — the Xenova vision_model.onnx outputs un-normalised projected embeddings. Cosine similarity still works without normalising (the math divides by norms) but pre-normalisation makes the resulting vectors interchangeable and the cache cosines well-conditioned.

## Obsolete / No Longer Relevant

The combined-graph code path with dummy text inputs (pre-2026-04-26). The `FilterType::Nearest` and ImageNet-stats shortcut documented in earlier versions of `notes/clip-preprocessing-decisions.md`. The 1.1 GB combined `model.onnx` filename `model_image.onnx` — files now use `clip_vision.onnx` and the migration system invalidates legacy data automatically.
