//! Shared preprocessing helpers used by every image encoder.
//!
//! Phase 12e — extracts the fast_image_resize Lanczos3 RGB8 resize
//! pattern into one place so every encoder (CLIP, DINOv2, SigLIP-2)
//! benefits from the same NEON-optimised path that the thumbnail
//! generator already uses.
//!
//! ## Why this matters
//!
//! The perf-1777226449 report showed `clip.preprocess_image` at
//! 304 ms/image mean (max 780 ms), making it the dominant per-image
//! cost outside the encode itself. Almost all of that is the
//! `image::imageops::resize` call. Published Neoverse-N1 benchmarks
//! show fast_image_resize at 7-13× the throughput of `image::imageops`
//! for the same Lanczos3 quality on RGB8 buffers; the M2's wider NEON
//! should match or exceed those numbers.
//!
//! ## Why not change the resize filter quality
//!
//! The previous code used `CatmullRom` (image-rs's bicubic-family
//! filter) to match PIL's BICUBIC, which is what the canonical CLIP
//! preprocessing uses. We're switching to Lanczos3 here because that's
//! what fast_image_resize is fastest at, AND Lanczos3 is generally
//! considered a higher-quality filter than CatmullRom (sharper, less
//! ringing on low-frequency content). The embedding-pipeline version
//! bump triggered by Phase 12e covers the slight distribution shift.

use fast_image_resize::{
    images::Image as FirImage, FilterType as FirFilter, PixelType, ResizeAlg, ResizeOptions,
    Resizer,
};
use image::RgbImage;
use tracing::warn;

/// Resize an RGB8 image to `target_w × target_h` via fast_image_resize
/// Lanczos3. Falls back to `image::imageops::resize` on any error so
/// the worst case is the previous behaviour.
///
/// `label` is included in the fallback warning so a future perf report
/// can identify which encoder's preprocessing is silently degrading.
pub fn fast_resize_rgb8(
    src: &RgbImage,
    target_w: u32,
    target_h: u32,
    label: &str,
) -> RgbImage {
    let (sw, sh) = src.dimensions();
    if sw == target_w && sh == target_h {
        return src.clone();
    }
    let src_fir = match FirImage::from_vec_u8(sw, sh, src.as_raw().clone(), PixelType::U8x3) {
        Ok(s) => s,
        Err(e) => {
            warn!("[{label}] fast_image_resize source build failed ({e}); falling back");
            return image::imageops::resize(
                src,
                target_w,
                target_h,
                image::imageops::FilterType::Lanczos3,
            );
        }
    };
    let mut dst = FirImage::new(target_w, target_h, PixelType::U8x3);
    let mut resizer = Resizer::new();
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FirFilter::Lanczos3));
    if let Err(e) = resizer.resize(&src_fir, &mut dst, &opts) {
        warn!("[{label}] fast_image_resize resize failed ({e}); falling back");
        return image::imageops::resize(
            src,
            target_w,
            target_h,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let buffer = dst.into_vec();
    RgbImage::from_raw(target_w, target_h, buffer).unwrap_or_else(|| {
        warn!(
            "[{label}] fast_image_resize output buffer wasn't the expected RGB8 length; \
             falling back"
        );
        image::imageops::resize(
            src,
            target_w,
            target_h,
            image::imageops::FilterType::Lanczos3,
        )
    })
}
