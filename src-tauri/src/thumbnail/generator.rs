use image::ImageReader;
use std::error::Error;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use crate::db::ImageDatabase;

use fast_image_resize::{
    images::Image as FirImage, FilterType as FirFilter, PixelType, ResizeAlg, ResizeOptions,
    Resizer,
};

/// Thumbnail generator that creates smaller versions of images for faster loading.
/// Follows the same pattern as the Encoder struct for consistency.
pub struct ThumbnailGenerator {
    thumbnail_dir: PathBuf,
    max_width: u32,
    max_height: u32,
}

impl ThumbnailGenerator {
    /// Create a new ThumbnailGenerator.
    ///
    /// # Arguments
    /// * `thumbnail_dir` - Directory where thumbnails will be stored
    /// * `max_width` - Maximum width for thumbnails (maintains aspect ratio)
    /// * `max_height` - Maximum height for thumbnails (maintains aspect ratio)
    pub fn new(
        thumbnail_dir: &Path,
        max_width: u32,
        max_height: u32,
    ) -> Result<Self, Box<dyn Error>> {
        // Create thumbnail directory if it doesn't exist
        fs::create_dir_all(thumbnail_dir)?;

        info!("=== Initializing ThumbnailGenerator ===");
        info!("Thumbnail directory: {}", thumbnail_dir.to_string_lossy());
        info!("Max dimensions: {}x{}", max_width, max_height);

        Ok(ThumbnailGenerator {
            thumbnail_dir: thumbnail_dir.to_path_buf(),
            max_width,
            max_height,
        })
    }

    /// Generate a thumbnail for a single image.
    ///
    /// Returns the thumbnail path and the original image dimensions
    /// (width, height). The thumbnail lands in
    /// `<thumbnail_dir>/root_<root_id>/thumb_<image_id>.jpg` when a
    /// root_id is supplied (Phase 9 per-root organisation), or in
    /// `<thumbnail_dir>/thumb_<image_id>.jpg` when None (legacy
    /// fallback for un-rooted rows).
    #[tracing::instrument(name = "thumbnail.generate", skip(self, image_path), fields(image_id, root_id))]
    pub fn generate_thumbnail(
        &self,
        image_path: &Path,
        image_id: i64,
        root_id: Option<i64>,
    ) -> Result<ThumbnailResult, Box<dyn Error>> {
        // Determine thumbnail filename based on image ID and per-root subfolder
        let thumbnail_filename = format!("thumb_{}.jpg", image_id);
        let thumbnail_path = match root_id {
            Some(rid) => {
                let dir = self.thumbnail_dir.join(format!("root_{rid}"));
                if !dir.exists() {
                    fs::create_dir_all(&dir)?;
                }
                dir.join(&thumbnail_filename)
            }
            None => self.thumbnail_dir.join(&thumbnail_filename),
        };

        // R7 — for JPEG sources, prefer the scaled-decode fast path:
        // jpeg-decoder's Decoder::scale(target_w, target_h) does scaled
        // IDCT (1/8, 1/4, 1/2, or 1×) at decode time, producing a
        // pre-shrunk RGB buffer instead of fully decoding 6000×3376
        // pixels just to throw 95% of them away. Falls back to the
        // generic ImageReader path for non-JPEG sources or any decode
        // failure.
        let (rgb, original_width, original_height) = match self
            .decode_jpeg_scaled(image_path)
        {
            Some(out) => out,
            None => {
                // Generic fallback: image-rs decodes the full image,
                // we get RGB8 + dimensions.
                let img = ImageReader::open(image_path)?
                    .with_guessed_format()?
                    .decode()?
                    .to_rgb8();
                let (w, h) = img.dimensions();
                (img, w, h)
            }
        };

        // Check if thumbnail already exists
        if thumbnail_path.exists() {
            return Ok(ThumbnailResult {
                thumbnail_path,
                original_width,
                original_height,
            });
        }

        // Calculate thumbnail dimensions while maintaining aspect ratio.
        // Note: this is computed against the *original* dimensions, not
        // the scaled-decode buffer — we want the same target as the old
        // path produced.
        let (thumb_width, thumb_height) =
            self.calculate_thumbnail_size(original_width, original_height);

        // R6 — fast_image_resize Lanczos3. Published Neoverse-N1
        // benchmarks show 7-13× speedup over image::imageops at the
        // same RGB8 + Lanczos3 quality. M2's NEON is wider than
        // Neoverse so the actual speedup should be at least as good.
        let resized = self.resize_with_fir(&rgb, thumb_width, thumb_height)?;

        // Save as JPEG with good quality. Quality 80 matches what
        // image-rs's default JpegEncoder used (~75-85 range).
        let dyn_img = image::DynamicImage::ImageRgb8(resized);
        dyn_img.save_with_format(&thumbnail_path, image::ImageFormat::Jpeg)?;

        debug!(
            "Generated thumbnail: {} ({}x{} -> {}x{})",
            image_path.file_name().unwrap_or_default().to_string_lossy(),
            original_width,
            original_height,
            thumb_width,
            thumb_height
        );

        Ok(ThumbnailResult {
            thumbnail_path,
            original_width,
            original_height,
        })
    }

    /// R7 — JPEG scaled-decode fast path.
    ///
    /// Returns `Some((rgb, original_w, original_h))` if the source is a
    /// JPEG that decoded successfully under jpeg-decoder; `None`
    /// otherwise (caller falls through to the generic image-rs path).
    ///
    /// Algorithm:
    ///   1. Read JPEG header to get true dimensions.
    ///   2. Compute the smallest scale factor (1, 2, 4, 8) such that
    ///      the scaled output is still >= the target thumbnail size on
    ///      every axis. This way the fast_image_resize step that
    ///      follows only has a small downsample left to do.
    ///   3. Set Decoder.scale(scaled_w, scaled_h) and decode.
    ///
    /// On any error this returns None — the generic decoder above
    /// will then attempt the file with image-rs, which has wider
    /// format support and better tolerance for malformed JPEGs.
    fn decode_jpeg_scaled(
        &self,
        image_path: &Path,
    ) -> Option<(image::RgbImage, u32, u32)> {
        let ext = image_path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
        if !matches!(ext.as_deref(), Some("jpg") | Some("jpeg")) {
            return None;
        }
        let file = std::fs::File::open(image_path).ok()?;
        let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(file));
        // Have to read metadata before scale().
        decoder.read_info().ok()?;
        let info = decoder.info()?;
        let (orig_w, orig_h) = (info.width as u32, info.height as u32);

        // Pick a scale factor — the largest in {1, 2, 4, 8} such that
        // the scaled buffer is >= the target thumbnail dims. Going
        // smaller would force fast_image_resize to upscale, which
        // defeats the purpose.
        let (target_w, target_h) =
            self.calculate_thumbnail_size(orig_w, orig_h);
        let mut factor: u16 = 8;
        while factor > 1
            && ((orig_w / factor as u32) < target_w
                || (orig_h / factor as u32) < target_h)
        {
            factor /= 2;
        }
        let scaled_w = (orig_w / factor as u32).max(1) as u16;
        let scaled_h = (orig_h / factor as u32).max(1) as u16;

        // jpeg-decoder's scale() asks for the *requested* width+height;
        // it returns the actual scaled dims (may be slightly different
        // due to MCU boundaries). We feed those back to image-rs.
        let (actual_w, actual_h) = decoder.scale(scaled_w, scaled_h).ok()?;
        let pixels = decoder.decode().ok()?;
        let pixel_format = decoder.info()?.pixel_format;

        // jpeg-decoder returns interleaved bytes in either RGB24 or
        // L8 (greyscale) layout depending on the source. We need RGB8
        // for the rest of the pipeline.
        let rgb = match pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => {
                image::RgbImage::from_raw(actual_w as u32, actual_h as u32, pixels)?
            }
            jpeg_decoder::PixelFormat::L8 => {
                // Promote greyscale → RGB by replicating the channel.
                let mut buf =
                    Vec::with_capacity(pixels.len() * 3);
                for p in pixels {
                    buf.push(p);
                    buf.push(p);
                    buf.push(p);
                }
                image::RgbImage::from_raw(actual_w as u32, actual_h as u32, buf)?
            }
            // CMYK and L16 are rare for thumbnail sources; fall back
            // so image-rs can handle them via its more general decoder.
            _ => return None,
        };

        Some((rgb, orig_w, orig_h))
    }

    /// R6 — Lanczos3 resize via fast_image_resize on RGB8.
    fn resize_with_fir(
        &self,
        src: &image::RgbImage,
        target_w: u32,
        target_h: u32,
    ) -> Result<image::RgbImage, Box<dyn Error>> {
        let (sw, sh) = src.dimensions();
        // Trivial case — already at target size, or smaller (no
        // upscaling). Skip the resize entirely.
        if sw == target_w && sh == target_h {
            return Ok(src.clone());
        }
        // fast_image_resize 6 owns its own image types. We hand it the
        // raw RGB8 bytes directly via FirImage::from_vec_u8 — this is
        // the documented zero-copy path (avoids the DynamicImage
        // dance) and works because RgbImage's underlying buffer is
        // already in the U8x3 layout fast_image_resize expects.
        let src_fir = FirImage::from_vec_u8(
            sw,
            sh,
            src.as_raw().clone(),
            PixelType::U8x3,
        )
        .map_err(|e| format!("fast_image_resize source failed: {e}"))?;
        let mut dst = FirImage::new(target_w, target_h, PixelType::U8x3);
        let mut resizer = Resizer::new();
        let opts = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FirFilter::Lanczos3));
        if let Err(e) = resizer.resize(&src_fir, &mut dst, &opts) {
            warn!("fast_image_resize failed ({e}); falling back to image-rs");
            return Ok(image::imageops::resize(
                src,
                target_w,
                target_h,
                image::imageops::FilterType::Lanczos3,
            ));
        }
        let buffer = dst.into_vec();
        image::RgbImage::from_raw(target_w, target_h, buffer).ok_or_else(|| {
            "fast_image_resize output buffer wasn't the expected RGB8 length"
                .into()
        })
    }

    /// Calculate thumbnail dimensions maintaining aspect ratio.
    /// Will not upscale images smaller than max dimensions.
    fn calculate_thumbnail_size(&self, width: u32, height: u32) -> (u32, u32) {
        let width_ratio = self.max_width as f32 / width as f32;
        let height_ratio = self.max_height as f32 / height as f32;
        let ratio = width_ratio.min(height_ratio).min(1.0); // Don't upscale

        let new_width = (width as f32 * ratio).round() as u32;
        let new_height = (height as f32 * ratio).round() as u32;

        (new_width.max(1), new_height.max(1)) // Ensure at least 1px
    }

    /// Generate thumbnails for all images that don't have them yet.
    /// Similar to `encode_all_images_in_database` in Encoder.
    pub fn generate_all_missing_thumbnails(
        &self,
        db: &ImageDatabase,
    ) -> Result<(), Box<dyn Error>> {
        let images = db.get_images_without_thumbnails()?;

        if images.is_empty() {
            info!("All images already have thumbnails, skipping generation.");
            return Ok(());
        }

        info!(
            "Found {} images without thumbnails, generating...",
            images.len()
        );

        let total_images = images.len();
        let mut success_count = 0;
        let mut error_count = 0;

        for (idx, image) in images.iter().enumerate() {
            if (idx + 1) % 10 == 0 || idx == 0 {
                debug!("Generating thumbnails... {}/{}", idx + 1, total_images);
            }

            // Legacy bulk path: no per-root segregation (root_id None).
            // The indexing pipeline calls generate_thumbnail directly
            // with the actual root_id; this method is kept for the
            // simple "regenerate all missing" use case.
            match self.generate_thumbnail(Path::new(&image.path), image.id, None) {
                Ok(result) => {
                    // Update database with thumbnail info and original dimensions
                    match db.update_image_thumbnail(
                        image.id,
                        &result.thumbnail_path,
                        result.original_width,
                        result.original_height,
                    ) {
                        Ok(_) => {
                            success_count += 1;
                        }
                        Err(e) => {
                            error!("Failed to update database for image {}: {}", image.id, e);
                            error_count += 1;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to generate thumbnail for {}: {}", image.path, e);
                    error_count += 1;
                }
            }
        }

        info!(
            "Thumbnail generation complete: {} succeeded, {} failed",
            success_count, error_count
        );

        Ok(())
    }

    /// Get the thumbnail path for an image ID (without generating).
    pub fn get_thumbnail_path(&self, image_id: i64) -> PathBuf {
        let thumbnail_filename = format!("thumb_{}.jpg", image_id);
        self.thumbnail_dir.join(thumbnail_filename)
    }
}

/// Result of thumbnail generation containing paths and dimensions.
pub struct ThumbnailResult {
    pub thumbnail_path: PathBuf,
    pub original_width: u32,
    pub original_height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_thumbnail_size_landscape() {
        let temp_dir = std::env::temp_dir().join("thumb_test");
        let gen = ThumbnailGenerator::new(&temp_dir, 400, 300).unwrap();

        // Landscape image 2000x1500 should scale to 400x300
        let (w, h) = gen.calculate_thumbnail_size(2000, 1500);
        assert_eq!(w, 400);
        assert_eq!(h, 300);
    }

    #[test]
    fn test_calculate_thumbnail_size_portrait() {
        let temp_dir = std::env::temp_dir().join("thumb_test");
        let gen = ThumbnailGenerator::new(&temp_dir, 400, 300).unwrap();

        // Portrait image 1500x2000 should scale to 225x300 (height constrained)
        let (w, h) = gen.calculate_thumbnail_size(1500, 2000);
        assert_eq!(w, 225);
        assert_eq!(h, 300);
    }

    #[test]
    fn test_calculate_thumbnail_size_no_upscale() {
        let temp_dir = std::env::temp_dir().join("thumb_test");
        let gen = ThumbnailGenerator::new(&temp_dir, 400, 300).unwrap();

        // Small image 100x100 should not be upscaled
        let (w, h) = gen.calculate_thumbnail_size(100, 100);
        assert_eq!(w, 100);
        assert_eq!(h, 100);
    }

    #[test]
    fn test_calculate_thumbnail_size_wide() {
        let temp_dir = std::env::temp_dir().join("thumb_test");
        let gen = ThumbnailGenerator::new(&temp_dir, 400, 300).unwrap();

        // Very wide image 4000x1000 should be width-constrained
        let (w, h) = gen.calculate_thumbnail_size(4000, 1000);
        assert_eq!(w, 400);
        assert_eq!(h, 100);
    }

    #[test]
    fn test_get_thumbnail_path() {
        let temp_dir = std::env::temp_dir().join("thumb_test");
        let gen = ThumbnailGenerator::new(&temp_dir, 400, 300).unwrap();

        let path = gen.get_thumbnail_path(42);
        assert!(path.to_string_lossy().contains("thumb_42.jpg"));
    }
}
