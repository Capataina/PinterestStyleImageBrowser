//! First-launch ONNX model + tokenizer download.
//!
//! Pass 4b scope: synchronous, foreground download to stdout — runs once
//! during app startup if the relevant files are missing from
//! `paths::models_dir()`. Pass 5 will move this to a background task with
//! progress events surfaced to the UI.
//!
//! ## Sources (verified 2026-04-25)
//!
//! - **Image encoder** — Xenova's HuggingFace Optimum ONNX export of
//!   OpenAI CLIP ViT-B/32. Combined-graph variant (both image and text
//!   branches in one model) so the existing `encoder.rs` pipeline that
//!   feeds dummy text inputs alongside the real image tensor works
//!   without modification. The signature matches:
//!     - inputs: `pixel_values` [1,3,224,224], `input_ids` [1,1], `attention_mask` [1,1]
//!     - output: `image_embeds` [1,512]
//!
//! - **Text encoder** — sentence-transformers' multilingual CLIP. This
//!   model is specifically trained to map 50+ languages into the same
//!   512-d embedding space as OpenAI's English CLIP, which is what makes
//!   typing `犬` find dogs. The Xenova text-only model is English-only
//!   and would NOT be a substitute here.
//!
//! - **Tokenizer** — the multilingual model's `tokenizer.json`. The
//!   pure-Rust WordPiece tokenizer in `encoder_text.rs` parses this file
//!   directly.
//!
//! Total first-launch download: ~1.15 GB. Subsequent launches no-op
//! because the files exist locally.

use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};

use std::path::Path;

use tracing::{debug, info};

use crate::paths;

/// Image encoder ONNX. Xenova/clip-vit-base-patch32 combined-graph export.
const IMAGE_MODEL_URL: &str =
    "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/onnx/model.onnx";

/// Multilingual text encoder ONNX. sentence-transformers/clip-ViT-B-32-multilingual-v1.
const TEXT_MODEL_URL: &str =
    "https://huggingface.co/sentence-transformers/clip-ViT-B-32-multilingual-v1/resolve/main/onnx/model.onnx";

/// Tokenizer for the multilingual text encoder (BERT-like vocab).
const TOKENIZER_URL: &str =
    "https://huggingface.co/sentence-transformers/clip-ViT-B-32-multilingual-v1/resolve/main/tokenizer.json";

/// Local filenames inside `paths::models_dir()`. These are the names the
/// rest of the codebase already expects (see `lib.rs::semantic_search`
/// and `main.rs`).
const IMAGE_MODEL_FILENAME: &str = "model_image.onnx";
const TEXT_MODEL_FILENAME: &str = "model_text.onnx";
const TOKENIZER_FILENAME: &str = "tokenizer.json";

/// Download every model file that is missing from `paths::models_dir()`.
/// Files that already exist are left alone — no version check, no hash
/// verification (yet — see TODO at the bottom of this file).
///
/// Returns Ok(()) when every required file is present after the call.
/// Returns Err on the first download that fails — partial state on disk
/// (a half-written .onnx file) is not cleaned up automatically; on the
/// next launch, the partial file's existence will skip the redownload
/// and the app will fail to load the model. Worth catching in Pass 5
/// when we move to async downloads with proper temp-file + rename.
pub fn download_models_if_missing() -> Result<(), Box<dyn Error>> {
    let models_dir = paths::models_dir();

    let targets = [
        (IMAGE_MODEL_URL, IMAGE_MODEL_FILENAME),
        (TEXT_MODEL_URL, TEXT_MODEL_FILENAME),
        (TOKENIZER_URL, TOKENIZER_FILENAME),
    ];

    let mut anything_downloaded = false;

    for (url, filename) in targets {
        let dest = models_dir.join(filename);
        if dest.exists() {
            continue;
        }
        if !anything_downloaded {
            info!(
                "First-launch model setup begun — files will land in {}",
                models_dir.display()
            );
            anything_downloaded = true;
        }
        download_to_file(url, &dest)?;
    }

    if anything_downloaded {
        info!("All model files present.");
    }
    Ok(())
}

/// Synchronous chunked download with stdout progress. Writes to
/// `dest.with_extension("part")` first then renames on success — that
/// way an interrupted download leaves a `.part` file behind that the
/// next run will see-and-delete-and-retry rather than treating as
/// complete (see partial-file note below).
fn download_to_file(url: &str, dest: &Path) -> Result<(), Box<dyn Error>> {
    info!("GET {url}");

    let resp = ureq::get(url).call()?;
    let total_bytes: Option<u64> = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let part_path = dest.with_extension("part");
    // If a previous run left a stale .part behind, remove it — its bytes
    // can't be reused without HTTP Range semantics, which we don't do yet.
    if part_path.exists() {
        let _ = fs::remove_file(&part_path);
    }

    let mut reader = resp.into_body().into_reader();
    let file = File::create(&part_path)?;
    let mut writer = BufWriter::new(file);

    // 256 KB chunks. Big enough that progress reporting isn't a fire hose
    // (a 600 MB file is ~2400 chunks); small enough that we surface
    // progress within a few seconds even on slow connections.
    let mut buf = vec![0u8; 256 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_logged_percent: i32 = -1;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        downloaded += n as u64;

        // Progress: log every full 10% step (or on every megabyte if
        // we don't know the total length).
        if let Some(total) = total_bytes {
            let percent = ((downloaded as f64 / total as f64) * 100.0) as i32;
            // Round down to nearest 10 so we get 0%, 10%, 20%, ..., 100%
            let bucket = (percent / 10) * 10;
            if bucket > last_logged_percent {
                last_logged_percent = bucket;
                debug!(
                    "  {bucket}% — {} / {} MB",
                    downloaded / 1_048_576,
                    total / 1_048_576
                );
            }
        } else if downloaded.is_multiple_of(10 * 1_048_576) {
            // No content-length — log every 10 MB.
            debug!("  {} MB so far", downloaded / 1_048_576);
        }
    }

    writer.flush()?;
    drop(writer);

    fs::rename(&part_path, dest)?;
    info!(
        "saved {} ({} MB)",
        dest.display(),
        downloaded / 1_048_576
    );

    Ok(())
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
}

// TODO(future hardening): Once Pass 5 lands the async download path:
// - Verify a SHA256 of each file against a hash bundled in this module.
//   HuggingFace exposes `X-Linked-Etag` headers that often contain the
//   blob's git-LFS SHA256, which we could pin and check.
// - Add HTTP Range resumption so an interrupted 600 MB download can
//   pick up where it left off. The current `.part` file is created
//   afresh on every retry.
// - Surface progress as a Tauri event so the UI can render a real
//   progress bar instead of relying on stdout.
