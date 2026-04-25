# clip-image-encoder

*Maturity: working*

## Scope / Purpose

Loads a CLIP ViT-B/32 ONNX model and produces 512-dimensional `f32` embeddings for image files. Tries CUDA first, falls back to CPU. Operates per image and in batches. Used at startup to populate every row in the `images` table that lacks an embedding.

## Boundaries / Ownership

- **Owns:** image preprocessing (resize, channel split, normalisation), ONNX session lifecycle, batched inference, the dummy-text-input hack required by the bundled model.
- **Does not own:** writing embeddings to disk (delegates to `database::update_image_embedding`), CUDA detection (relies on `ort`'s built-in fallback semantics).
- **Public API:** `Encoder::new(model_path)`, `inspect_model`, `preprocess_image`, `batch_preprocess_image`, `encode`, `encode_batch`, `encode_all_images_in_database(batch_size, &db)`.

## Current Implemented Reality

### Pipeline per image

```text
ImageReader::open → with_guessed_format → decode → to_rgb8 (224×224 with FilterType::Nearest)
    │
    ▼
Split RGB into 3 contiguous slices (R then G then B):
    [R0..R(224*224-1), G0..G(...), B0..B(...)]      # length 224*224*3
    │
    ▼
Normalise per-channel: x = (x/255 - mean[c]) / std[c]
    where mean = [0.485, 0.456, 0.406]   ← ImageNet stats, not CLIP-native
          std  = [0.229, 0.224, 0.225]
    │
    ▼
Reshape to ndarray::Array4 with shape (1, 3, 224, 224)
    │
    ▼
ort::Tensor::from_array((shape, raw_vec))
    │
    ▼
Session::run with three inputs:
    pixel_values    ← image tensor
    input_ids       ← dummy [[0]] (i64, shape [1, 1])
    attention_mask  ← dummy [[0]] (i64, shape [1, 1])
    │
    ▼
Extract output["image_embeds"] → Vec<f32> length 512
```

Source: `encoder.rs:62-195`.

### Batch path

`encode_batch` batches preprocessing in groups of 32 (hardcoded `preprocessing_batch_size = 32` at `encoder.rs:207`), concatenates each chunk along axis 0, runs ONNX once per chunk, splits the output by `embedding_size` (512). The `encode_all_images_in_database` caller passes the same `batch_size = 32` from `main.rs:49`.

### CUDA fallback

`Session::builder().with_execution_providers([CUDAExecutionProvider::default().build()])`. If that builder errors, falls back to a plain `Session::builder().commit_from_file(model_path)`. The fallback is **inside the `match` arm** — it does not test whether CUDA *actually* runs, only whether the builder accepted the provider config. Per the inline comments at `encoder.rs:25-33`, the author observed that debug builds use CPU even when CUDA is "enabled," and release builds use the GPU. This is documented but not solved.

### Hardcoded ONNX I/O names

```rust
let input_name  = "pixel_values";
let output_name = "image_embeds";
```

Plus the dummy text inputs are required because the bundled ONNX graph contains both image and text branches and would error on missing tensors. (`encoder.rs:158-167`.)

## Key Interfaces / Data Flow

```text
main.rs (startup)
    ──► Encoder::new(Path::new("models/model_image.onnx"))
    ──► encode_all_images_in_database(32, &database)
        ──► db.get_images_without_embeddings()
        ──► for batch in chunks(32):
              encode_batch(batch_paths)
                ──► batch_preprocess_image (chunked further by 32)
                ──► Session::run per ONNX-batch
                ──► split output by 512
              for (image, embedding) in zip:
                db.update_image_embedding(id, embedding)
```

Output BLOB encoding is owned by `database::update_image_embedding` (the `unsafe` cast).

## Implemented Outputs / Artifacts

- 512-d `Vec<f32>` per image, written into `images.embedding` BLOB column.
- Tests assert `embedding.len() == 512` and `0.9 < L2 norm < 1.1` (the model is approximately L2-normalised by training).

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `FilterType::Nearest` resize | Every image preprocessing | Lower-quality preprocessing than the reference CLIP implementation, which uses Bicubic/Lanczos. Embedding quality is degraded by an unmeasured but non-zero amount. One-line fix to `Lanczos3`. |
| ImageNet normalisation stats, not CLIP-native | Every image preprocessing | The reference OpenAI CLIP uses `mean=[0.48145466, 0.4578275, 0.40821073]`, `std=[0.26862954, 0.26130258, 0.27577711]`. The stats here are ImageNet's: `mean=[0.485, 0.456, 0.406]`, `std=[0.229, 0.224, 0.225]`. Drift in cosine similarity vs reference embeddings. |
| Dummy text-branch tensors required | Every inference call | If the model is swapped to one that *only* exposes the image branch, the call fails with "missing input." See author's TODO at `encoder.rs:160`. |
| Hardcoded ONNX input/output names | A model swap | Silent breakage — a different graph would not have `pixel_values`/`image_embeds` named exactly that way. There is no schema check at session-create time. |
| `ort = 2.0.0-rc.10` (release candidate) | Building the project at any time | Non-pinned RC, with `download-binaries` feature pulling CUDA libs at build time. Build is non-reproducible and tied to external hosting. The session-create panic during dev sometimes asks for a newer rc — caught only at runtime. |
| Encoder pipeline is single-threaded over batches | Initial encoding pass | Several minutes for hundreds of images. The batching reduces overhead vs per-image, but does not parallelise across CPU cores. |
| First-failed encode aborts the whole batch | A corrupt JPEG in `test_images/` | `encode_all_images_in_database` propagates the first `Err(?)` and stops. Subsequent images are not attempted. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Swap `FilterType::Nearest` → `FilterType::Lanczos3` (one-character change at `encoder.rs:69`).
- Swap ImageNet stats → CLIP-native stats (six-number change at `encoder.rs:91-92`).
- Add a model-swap-friendly inspector that checks input/output names at session creation and errors clearly if the bundled model does not match expectations.
- Consider parallelising preprocessing via `rayon` while keeping ONNX inference serialised (the session is `&mut self` so it cannot be shared across threads without further work).
- Track embedding quality vs a Python reference CLIP via a small benchmark suite. The `test_inference_speed` test already provides a CPU/GPU detection signal at runtime; a quality test would close the loop.

## Durable Notes / Discarded Approaches

The author left several rich rationale comments in code that are worth preserving here:

- **CUDA detection is structurally limited by `ort`'s API** (`encoder.rs:25-33`): "the session says it's initialised with CUDA but the encoding is incredibly slow ... so we do a test run here ... it's definitely using the CPU not GPU despite saying it initialised with the GPU ... it seems like during debug, the PC uses the CPU even if CUDA is enabled but in release mode it uses the GPU." The fallback is intentional defensive code, not paranoia.
- **The ONNX RGB-channel layout transform is required** (`encoder.rs:74-75`): "Change layout from RGBRGB... to RRR...GGG...BBB... for some reason ONNX wants it like this." This is in fact CHW (channels-first) layout, which is the standard for PyTorch-exported models. The comment is informal but the work is correct.
- **The dummy text-branch tensors are a model contract, not a workaround** (`encoder.rs:158-167`): "Create dummy inputs for the text branch to prevent ONNX crashes ... The model expects these to exist even if we are only doing image encoding." A future model that exposes only the image branch would let this be deleted, but the bundled model needs it. The author's `// TODO: Fix this when it breaks in the future...` is honest.
- **The `try_extract_tensor` method choice was forced** (`encoder.rs:187-191`): "this is an absolute pain to fix ... if we get a tuple from the tensor, tuples do not have view, as_slice or any of the other methods for us to turn them into a vec ... so we use try_extract_tensor which gives us both the shape and a view we can turn into a vec." Documented friction with the `ort` API.
- **The ndarray ownership/Tensor::from_array dance** (`encoder.rs:146-151`): the `into_raw_vec_and_offset()` extraction is a workaround for the lifetime constraints of `Tensor::from_array`. Author's TODO is "Optimize this to avoid copies." Today's implementation copies the preprocessed array to construct the Tensor; a zero-copy version requires a different ort API path.

## Obsolete / No Longer Relevant

The earlier per-image `encode_batch` implementation that called `preprocess_image` in a loop was replaced in commit `33398fb` (2025-12-08) with a `batch_preprocess_image`-based pipeline. The TODO it cleared is gone from the source.
