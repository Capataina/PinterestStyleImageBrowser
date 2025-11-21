use std::path::{Path};
use image_browser_lib::image_struct::ImageStruct;

const SUPPORTED_IMAGE_EXTENSIONS: [&str; 7] = ["jpg", "png", "gif", "jpeg", "bmp", "tiff", "webp"];

fn is_supported_image(path: &Path) -> bool {
    if let Some(extension) = std::path::Path::new(path).extension() {
        if let Some(ext_str) = extension.to_str() {
            return SUPPORTED_IMAGE_EXTENSIONS.contains(&ext_str.to_lowercase().as_str());
        }
    }
    false
}

pub struct ImageScanner{

}


impl ImageScanner {
    pub fn new() -> Self {
        ImageScanner {}
    }

    pub fn scan_directory(&self, root: &Path) -> Result<Vec<ImageStruct>, std::io::Error> {
        let mut images: Vec<ImageStruct> = Vec::new();

        for entry_res in std::fs::read_dir(root)? {
            let entry = entry_res?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                let mut nested = self.scan_directory(&path)?;
                images.append(&mut nested);
            } else if file_type.is_file() {
                if is_supported_image(&path) {
                    images.push(ImageStruct::new(&path, Vec::new()));
                }
            }
        }

        Ok(images)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_scan_directory_finds_all_images() {
        let test_dir = Path::new("test_images");
        
        let scanner = ImageScanner::new();
        let results = scanner.scan_directory(test_dir).unwrap();
        
        assert_eq!(results.len(), 4, "Should find exactly 4 image files");
    }

    #[test]
    fn test_supported_extensions() {
        assert!(is_supported_image(Path::new("photo.jpg")));
        assert!(is_supported_image(Path::new("image.PNG"))); // case insensitive
        assert!(!is_supported_image(Path::new("document.pdf")));
        assert!(!is_supported_image(Path::new("video.mp4")));
    }
}