use image::ImageReader;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

use crate::db::ImageDatabase;

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
    /// Returns the thumbnail path and the original image dimensions (width, height).
    pub fn generate_thumbnail(
        &self,
        image_path: &Path,
        image_id: i64,
    ) -> Result<ThumbnailResult, Box<dyn Error>> {
        // Determine thumbnail filename based on image ID
        let thumbnail_filename = format!("thumb_{}.jpg", image_id);
        let thumbnail_path = self.thumbnail_dir.join(&thumbnail_filename);

        // Load the original image to get dimensions
        let img = ImageReader::open(image_path)?
            .with_guessed_format()?
            .decode()?;
        let (original_width, original_height) = (img.width(), img.height());

        // Check if thumbnail already exists
        if thumbnail_path.exists() {
            return Ok(ThumbnailResult {
                thumbnail_path,
                original_width,
                original_height,
            });
        }

        // Calculate thumbnail dimensions while maintaining aspect ratio
        let (thumb_width, thumb_height) =
            self.calculate_thumbnail_size(original_width, original_height);

        // Resize the image using high-quality Lanczos3 filter
        let thumbnail = img.thumbnail(thumb_width, thumb_height);

        // Save as JPEG with good quality
        thumbnail.save_with_format(&thumbnail_path, image::ImageFormat::Jpeg)?;

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

            match self.generate_thumbnail(Path::new(&image.path), image.id) {
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
