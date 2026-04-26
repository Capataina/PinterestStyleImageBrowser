//! First-launch ONNX model + tokenizer download with live progress.
//!
//! Files land in `paths::models_dir()` and are skipped if already present.
//! A caller-supplied `Progress` callback receives running totals after a
//! HEAD-request preflight establishes the aggregate target size; the
//! indexing thread wires this through to the `indexing-progress` Tauri
//! event channel so the frontend pill can render a smooth bar across all
//! three files.
//!
//! ## Sources (verified 2026-04-25)
//!
//! - **Image encoder** — Xenova's HuggingFace Optimum ONNX export of
//!   OpenAI CLIP ViT-B/32. Combined-graph variant with the signature:
//!     - inputs: `pixel_values` [1,3,224,224], `input_ids` [1,1], `attention_mask` [1,1]
//!     - output: `image_embeds` [1,512]
//!
//! - **Text encoder** — sentence-transformers' multilingual CLIP
//!   (clip-ViT-B-32-multilingual-v1). 50+ languages mapped into the
//!   shared 512-d CLIP embedding space.
//!
//! - **Tokenizer** — the multilingual model's `tokenizer.json`.
//!
//! Total first-launch download: ~1.15 GB.

use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use tracing::{debug, info, warn};

use crate::paths;

use crate::similarity_and_semantic_search::encoder_dinov2;
use crate::similarity_and_semantic_search::encoder_siglip2;

/// Image encoder ONNX. Xenova/clip-vit-base-patch32 combined-graph export.
const IMAGE_MODEL_URL: &str =
    "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/onnx/model.onnx";

/// Multilingual text encoder ONNX. sentence-transformers/clip-ViT-B-32-multilingual-v1.
const TEXT_MODEL_URL: &str =
    "https://huggingface.co/sentence-transformers/clip-ViT-B-32-multilingual-v1/resolve/main/onnx/model.onnx";

/// Tokenizer for the multilingual text encoder (BERT-like vocab).
const TOKENIZER_URL: &str =
    "https://huggingface.co/sentence-transformers/clip-ViT-B-32-multilingual-v1/resolve/main/tokenizer.json";

const IMAGE_MODEL_FILENAME: &str = "model_image.onnx";
const TEXT_MODEL_FILENAME: &str = "model_text.onnx";
const TOKENIZER_FILENAME: &str = "tokenizer.json";

/// Callback signature for download progress.
///
/// `processed` and `total` are aggregate byte counts across all
/// downloads in the current call; `current_file` is the filename
/// being fetched right now (or `None` if no file-specific work is
/// happening, e.g. during the HEAD-request preflight).
pub type ProgressFn = dyn Fn(u64, u64, Option<&str>) + Send + Sync;

/// Download every model file that is missing from `paths::models_dir()`.
/// Already-present files are left alone.
///
/// `progress` is invoked with running aggregate byte totals as bytes
/// land. The callback is also invoked once at the start with
/// `(0, total_bytes_to_download, None)` after the HEAD preflight, so
/// the UI can show the eventual size before any actual download begins.
#[tracing::instrument(name = "model_download.all", skip(progress))]
pub fn download_models_if_missing<F>(progress: F) -> Result<(), Box<dyn Error>>
where
    F: Fn(u64, u64, Option<&str>) + Send + Sync + 'static,
{
    let models_dir = paths::models_dir();

    // CLIP (legacy default) + SigLIP-2 (new default text+image) +
    // DINOv2 (image-only "View Similar" specialist).
    //
    // All seven files are downloaded eagerly at first launch so every
    // encoder choice in the Settings picker "just works" without
    // mid-session downloads. Total size on disk: ~2.1GB. The progress
    // callback shows aggregate bytes across all files for one smooth
    // determinate bar.
    //
    // If a URL 404s, the user gets a download error and can update the
    // const to the right HF path. Each encoder family's URLs live in
    // its own module (see encoder_siglip2.rs, encoder_dinov2.rs) so
    // fixes are localised.
    let targets = [
        // CLIP family (existing — image, text, tokenizer)
        (IMAGE_MODEL_URL, IMAGE_MODEL_FILENAME),
        (TEXT_MODEL_URL, TEXT_MODEL_FILENAME),
        (TOKENIZER_URL, TOKENIZER_FILENAME),
        // SigLIP-2 family (new — image, text, tokenizer)
        (
            encoder_siglip2::SIGLIP2_IMAGE_MODEL_URL,
            encoder_siglip2::SIGLIP2_IMAGE_MODEL_FILENAME,
        ),
        (
            encoder_siglip2::SIGLIP2_TEXT_MODEL_URL,
            encoder_siglip2::SIGLIP2_TEXT_MODEL_FILENAME,
        ),
        (
            encoder_siglip2::SIGLIP2_TOKENIZER_URL,
            encoder_siglip2::SIGLIP2_TOKENIZER_FILENAME,
        ),
        // DINOv2 (new — image only, no text encoder, no tokenizer)
        (
            encoder_dinov2::DINOV2_IMAGE_MODEL_URL,
            encoder_dinov2::DINOV2_IMAGE_MODEL_FILENAME,
        ),
    ];

    // Phase 1: figure out which files are missing and how big they are
    // in total. This lets the caller's progress bar be determinate
    // across the whole 1+GB download rather than per-file.
    let mut to_download: Vec<(&str, &str, u64)> = Vec::new();
    for (url, filename) in targets {
        let dest = models_dir.join(filename);
        if dest.exists() {
            continue;
        }
        let size = head_content_length(url).unwrap_or(0);
        to_download.push((url, filename, size));
    }

    if to_download.is_empty() {
        return Ok(());
    }

    info!(
        "First-launch model setup begun — {} files to fetch into {}",
        to_download.len(),
        models_dir.display()
    );

    let total_bytes: u64 = to_download.iter().map(|(_, _, s)| s).sum();
    progress(0, total_bytes, None);

    // Phase 2: actually download. The aggregate counter accumulates
    // across files so the UI sees one smooth 0..total progression.
    let mut bytes_so_far: u64 = 0;
    for (url, filename, _) in &to_download {
        let dest = models_dir.join(filename);
        download_to_file(url, &dest, &mut bytes_so_far, total_bytes, &progress)?;
    }

    info!("All model files present.");
    progress(total_bytes, total_bytes, None);
    Ok(())
}

/// HEAD a URL to read its Content-Length. Falls through to None on any
/// error; the caller treats unknown sizes as zero in the aggregate
/// total (so the bar is slightly less accurate but still progresses).
#[tracing::instrument(name = "model_download.head")]
fn head_content_length(url: &str) -> Option<u64> {
    let resp = ureq::head(url).call().ok()?;
    resp.headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Synchronous chunked download with progress callback.
///
/// `bytes_so_far` is the running aggregate counter — we mutate it in
/// place so the caller's tally stays accurate across files. Writes to
/// `dest.with_extension("part")` first then renames atomically on
/// success; an interrupted download leaves a stale `.part` that the
/// next run cleans up before retrying.
#[tracing::instrument(name = "model_download.file", skip(bytes_so_far, progress))]
fn download_to_file(
    url: &str,
    dest: &Path,
    bytes_so_far: &mut u64,
    total_bytes: u64,
    progress: &ProgressFn,
) -> Result<(), Box<dyn Error>> {
    let filename = dest
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("model");
    info!("GET {url}");

    let resp = ureq::get(url).call()?;
    let part_path = dest.with_extension("part");

    // If a previous run left a stale .part behind, remove it — its
    // bytes can't be reused without HTTP Range semantics.
    if part_path.exists() {
        let _ = fs::remove_file(&part_path);
    }

    let mut reader = resp.into_body().into_reader();
    let file = File::create(&part_path)?;
    let mut writer = BufWriter::new(file);

    // 256 KB read chunks. Progress callback invoked at most every
    // ~512 KB written (one byte-counter update per loop) — that's
    // ~2000 callbacks for a 1 GB download, comfortably below
    // overhead concerns even with the Tauri event hop.
    let mut buf = vec![0u8; 256 * 1024];
    let mut last_emitted_bucket: i32 = -1;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        *bytes_so_far += n as u64;

        // Coalesce the progress callback to once per ~1% of the total
        // (so the UI gets ~100 events for a full download, smooth
        // enough for a determinate bar without thrashing).
        if total_bytes > 0 {
            let bucket = ((*bytes_so_far as f64 / total_bytes as f64) * 100.0) as i32;
            if bucket > last_emitted_bucket {
                last_emitted_bucket = bucket;
                progress(*bytes_so_far, total_bytes, Some(filename));
            }
        } else {
            // No total known — emit on every chunk write so the UI
            // sees the byte counter advance, even if it can't draw a
            // determinate bar.
            progress(*bytes_so_far, 0, Some(filename));
        }

        // Per-10% trace logging is independent of the UI callback.
        // Keeps the terminal log human-friendly without firehose-y
        // per-chunk lines.
        if let Some(total) = Some(total_bytes).filter(|t| *t > 0) {
            let pct = ((*bytes_so_far as f64 / total as f64) * 100.0) as i32;
            if pct % 10 == 0 && (pct / 10) * 10 != last_emitted_bucket / 10 * 10 {
                debug!(
                    "  {pct}% — {} / {} MB ({})",
                    *bytes_so_far / 1_048_576,
                    total / 1_048_576,
                    filename
                );
            }
        }
    }

    writer.flush()?;
    drop(writer);

    fs::rename(&part_path, dest)?;
    info!("saved {} ({} bytes)", dest.display(), file_size(dest));
    Ok(())
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or_else(|_| {
        warn!("could not stat {}", path.display());
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_constants_are_well_formed() {
        for url in [IMAGE_MODEL_URL, TEXT_MODEL_URL, TOKENIZER_URL] {
            assert!(
                url.starts_with("https://huggingface.co/"),
                "URL must point at HuggingFace, got {url}"
            );
            assert!(
                url.contains("/resolve/main/"),
                "URL must use the `resolve/main/` direct-download form, got {url}"
            );
        }
    }

    #[test]
    fn test_filenames_match_what_the_rest_of_the_codebase_expects() {
        // These names are referenced from main.rs (image model) and
        // lib.rs::semantic_search (text model + tokenizer). If any
        // filename here changes, those callers break.
        assert_eq!(IMAGE_MODEL_FILENAME, "model_image.onnx");
        assert_eq!(TEXT_MODEL_FILENAME, "model_text.onnx");
        assert_eq!(TOKENIZER_FILENAME, "tokenizer.json");
    }

    #[test]
    fn test_progress_signature_compiles() {
        // Compile-time only: ensures the closure shape we pass from
        // indexing.rs matches what download_models_if_missing expects.
        // Doesn't actually fetch anything.
        let _f = |_processed: u64, _total: u64, _file: Option<&str>| {};
        // Confirms F's bounds: Fn(u64, u64, Option<&str>) + Send + Sync
        fn assert_fn<F: Fn(u64, u64, Option<&str>) + Send + Sync>(_: F) {}
        assert_fn(|_a, _b, _c| {});
    }
}
