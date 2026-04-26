# model-download

*Maturity: working*

## Scope / Purpose

First-launch download manager for the eight ONNX / tokenizer files across three encoder families (CLIP, DINOv2, SigLIP-2). Pulls each file from its corresponding HuggingFace URL and writes them into `Library/models/`. Skips files that already exist on disk. Supports a per-byte progress callback so the indexing pipeline's status pill can render a determinate bar across the ~2.5 GB of total downloads instead of a "Checking models..." flash followed by a multi-minute silent stretch.

Per-encoder URL constants live in their respective encoder modules (`encoder_dinov2.rs`, `encoder_siglip2.rs`) — `model_download.rs` owns only the CLIP constants. The download function imports the per-encoder constants and treats all eight files uniformly.

## Boundaries / Ownership

- **Owns:** the three URL constants, the per-file download loop, the HEAD-preflight aggregate sizing, the chunked-GET stream-to-file write, the `ProgressFn` callback dispatch, the destination filenames.
- **Does not own:** where models live (delegates to `paths::models_dir()`), the indexing pipeline that calls this (`indexing.rs::run_pipeline_inner` Phase 1), the lazy fallback init for the text encoder (lives in `commands::semantic`).
- **Public API:** `download_models_if_missing<F>(progress: F) -> Result<(), Box<dyn Error>>` where `F: Fn(u64, u64, Option<&str>) + Send + Sync + 'static`, plus the type alias `ProgressFn`.

## Current Implemented Reality

### Sources (verified 2026-04-26 by parallel research agents — all FP32, all 200 OK)

| File | Encoder | URL | Size |
|------|---------|-----|------|
| `clip_vision.onnx` | CLIP image | `Xenova/clip-vit-base-patch32/resolve/main/onnx/vision_model.onnx` | ~352 MB |
| `clip_text.onnx` | CLIP text | `Xenova/clip-vit-base-patch32/resolve/main/onnx/text_model.onnx` | ~254 MB |
| `clip_tokenizer.json` | CLIP tokenizer | `Xenova/clip-vit-base-patch32/resolve/main/tokenizer.json` | ~2 MB |
| `dinov2_base_image.onnx` | DINOv2 image | `Xenova/dinov2-base/resolve/main/onnx/model.onnx` | ~347 MB |
| `siglip2_vision.onnx` | SigLIP-2 image | `onnx-community/siglip2-base-patch16-256-ONNX/resolve/main/onnx/vision_model.onnx` | ~372 MB |
| `siglip2_text.onnx` | SigLIP-2 text | `onnx-community/siglip2-base-patch16-256-ONNX/resolve/main/onnx/text_model.onnx` | ~1.13 GB |
| `siglip2_tokenizer.json` | SigLIP-2 tokenizer (Gemma SP) | `onnx-community/siglip2-base-patch16-256-ONNX/resolve/main/tokenizer.json` | ~34 MB |

**Total first-launch download: ~2.5 GB.** All FP32 — quantized variants are explicitly rejected on quality grounds (the user's stated preference; revisit only if a use case justifies the trade-off).

The CLIP filenames live in `model_download.rs` itself as `pub const CLIP_VISION_FILENAME` etc. so callers can reference them via `crate::model_download::CLIP_VISION_FILENAME`. The DINOv2 and SigLIP-2 constants live in their respective encoder modules (`encoder_dinov2::DINOV2_IMAGE_MODEL_FILENAME`, `encoder_siglip2::SIGLIP2_*_FILENAME`) — same pattern, different home, so per-encoder fixes are localised.

CLIP uses the **separate** `vision_model.onnx` + `text_model.onnx` exports, NOT the combined-graph `model.onnx` from before 2026-04-26. That swap was tied to the multilingual → English text-encoder swap (the multilingual distillation lived in a different embedding space than the image branch — root cause of "blue fish → Tristana" failure). Both halves were swapped together to keep the embedding space consistent. The `migrate_embedding_pipeline_version` (DB version 2) wipes legacy CLIP embeddings on first launch under the new code so the next indexing pass re-encodes everything cleanly.

### Two-phase execution

```rust
pub fn download_models_if_missing<F>(progress: F) -> Result<(), Box<dyn Error>>
where F: Fn(u64, u64, Option<&str>) + Send + Sync + 'static
{
    // Phase 1: HEAD preflight to compute total bytes
    //   - For each missing file, HEAD request to read Content-Length
    //   - Aggregate into total_bytes
    //   - One callback invocation: progress(0, total_bytes, None)
    //
    // Phase 2: Per-file chunked download
    //   - For each missing file, GET stream
    //   - Read in chunks (typical 16-64 KB), write to file via BufWriter
    //   - After each chunk, update running total and call progress(processed, total, Some(filename))
    //   - Renames .part to final on success
}
```

The HEAD preflight is the reason the UI can show a meaningful progress bar before any actual GET starts. Without it, the user would see "Downloading model_image.onnx — 234 MB" with no idea whether 234 is 50% or 5%. With it, the indexing pipeline emits `Phase::ModelDownload(0, 1153023488, None)` immediately and updates determinately.

### Per-file resilience

Files are written to `<filename>.part` during download, then renamed on completion. The `.part` extension is gitignored so a failed mid-download doesn't end up in a commit. The next launch sees no final file and re-downloads from scratch — there's no resume support yet.

### Tracing instrumentation

```rust
#[tracing::instrument(name = "model_download.all", skip(progress))]
pub fn download_models_if_missing<F>(progress: F) -> ...

// Plus per-file inner spans:
//   model_download.head — per-file Content-Length probe
//   model_download.file — per-file chunked download
```

The on-exit perf report can break a slow first-launch into "this file took longer than that file" with mean/p95 across the three downloads, and shows how much wallclock is preflight-HEAD vs actual GET stream.

### Error handling

The indexing pipeline calls `download_models_if_missing` with a `warn!`-on-error pattern:

```rust
if let Err(e) = model_download::download_models_if_missing(progress_cb) {
    warn!("model download skipped: {e}");
    emit(app, Phase::ModelDownload, 0, 0, Some(format!("Model download skipped: {e}")));
}
```

The pipeline continues even if the download fails. The thumbnail and (no-op) encode phases still run. Semantic search is unavailable until the user gets the models on disk by some other means (manual placement, retrying after a network fix). The lazy-init path in `commands::semantic` emits `ApiError::TextModelMissing(path)` on the first user query, giving the frontend a typed signal to show a re-download prompt.

## Key Interfaces / Data Flow

### Inputs

| Source | Provides |
|--------|----------|
| `paths::models_dir()` | Destination directory |
| HuggingFace HTTPS (3 URLs) | The model files |
| `progress: F` | Caller-supplied callback for per-byte updates |

### Outputs

| Destination | What |
|-------------|------|
| `Library/models/clip_vision.onnx` | ~352 MB |
| `Library/models/clip_text.onnx` | ~254 MB |
| `Library/models/clip_tokenizer.json` | ~2 MB |
| `Library/models/dinov2_base_image.onnx` | ~347 MB |
| `Library/models/siglip2_vision.onnx` | ~372 MB |
| `Library/models/siglip2_text.onnx` | ~1.13 GB |
| `Library/models/siglip2_tokenizer.json` | ~34 MB |
| `progress(processed_bytes, total_bytes, current_filename)` callback | One call after HEAD preflight, then one per chunk for each file |

The download function is now per-file fail-soft (see commit `e7a5a76` and the 2026-04-26 changes): a single 401/404/timeout no longer aborts the batch — each file's failure is logged with its URL so the user can identify which one needs a corrected URL. The aggregate counter still advances by the failed file's declared size so the progress bar doesn't stall mid-flight.

### Return shape

`Result<(), Box<dyn Error>>` — a generic boxed error because the function might surface I/O errors, HTTP errors, header parse failures, or any combination. Callers (only the indexing pipeline) format the message into a user-visible event.

## Implemented Outputs / Artifacts

- Three ONNX/tokenizer files in `Library/models/` after first successful run
- Tracing spans `model_download.all`, `model_download.head`, `model_download.file` visible in profile reports
- The `Phase::ModelDownload` events emitted by the wrapping indexing pipeline carry both the running byte count and the filename string

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Not resumable | Network failure mid-1 GB download | The `.part` file is left on disk; the next launch sees no final file and re-downloads from byte 0. For a 1 GB image encoder this is a real cost. |
| URL changes upstream | HuggingFace renames the repo or moves a file | The constants need a code change. There's no graceful fallback to a different source. |
| HEAD preflight may return 0 / wrong content-length | Some HTTP servers don't send Content-Length | Total stays 0 → the progress bar is indeterminate but the download still completes. |
| Per-file fail-soft means a missing encoder is silent at user-visible scale | One file 404s; the other 7 succeed; user picks the missing encoder in Settings | Indexing skips that encoder's pass with a `warn` log; semantic/View-Similar through that encoder returns 0 results. Mitigated by the `cosine_cache_populated` diagnostic which surfaces the per-encoder cache size in the on-exit perf report. |
| HuggingFace requires no auth for these URLs (they're public sentence-transformers / Xenova exports) | Future model swap to a gated repo | Would need API key threading. Today's flow assumes public hosting. |
| BufWriter is used but no explicit fsync after rename | Power loss mid-write | The `.part` is partial; next launch re-downloads. The rename is atomic-enough on every modern filesystem the app realistically runs on. |
| No retry / backoff | Transient network blip | Single-shot per file. Would benefit from a small retry loop for 5xx / connection-reset errors. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Resumable downloads via HTTP Range headers.** Honour the `.part` file length, send `Range: bytes=N-` to resume, fall back to full download if the server doesn't support Range. Materially changes the UX for users on slower or flakier connections.
- **Configurable model URLs.** A user might want to point at a self-hosted mirror or a quantised variant. Today the URLs are hardcoded. A `Library/models/manifest.json` could opt into custom sources.
- **Retry with exponential backoff** for transient errors (5xx, connection reset, DNS hiccup).
- **Checksum verification.** HuggingFace publishes sha256 sums in the file metadata; verify after download to detect corruption / MITM.
- **Quantised model variants.** The `enhancements/recommendations/06-int8-quantisation-encoders.md` document discusses int8 ONNX exports that would shrink the download by ~4× and speed up inference. Would change the URLs.

## Durable Notes / Discarded Approaches

- **`ureq` over `reqwest` for the HTTP client.** `ureq` is sync (matches the rest of the indexing pipeline) and has a much smaller dependency tree. The HEAD preflight + chunked GET pattern is straightforward without async.
- **HEAD preflight specifically because the UI needs determinate progress.** The original "Checking models..." flash followed by a silent multi-minute wait was the worst possible first-launch UX. The HEAD probes add a fraction of a second to the start of the download phase but make the bar meaningful immediately.
- **`.part` extension over a hidden tmpfile** because the user might inspect the directory mid-download and a clearly-named `.part` file communicates "this is in progress" better than a `.tmp_random_suffix` file.
- **Single HEAD per file rather than a manifest JSON.** A manifest would let one HTTP fetch return all three sizes; HuggingFace doesn't expose one for this set, and three HEADs is cheap. If a future feature needs N models the trade-off would flip.
- **The indexing pipeline owns the calling site, not commands.** A "force re-download" Tauri command was considered but not added — if the user wants to force re-download they can `rm -f Library/models/*` and restart, which is rare enough that an explicit command isn't justified yet. Adding one is trivial when the need arises.

## Obsolete / No Longer Relevant

The pre-Phase-4b layout assumed users would manually place model files into the `models/` directory. The README still mentions "user-supplied" in places that haven't been updated; in practice every fresh install auto-downloads.
