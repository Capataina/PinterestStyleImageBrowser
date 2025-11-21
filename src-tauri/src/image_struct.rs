use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageStruct {
    pub name: String,
    pub path: String,
    pub tags: Vec<String>,
}

impl ImageStruct {
    pub fn new(path: &Path, tags: Vec<String>) -> Self {

        let path_str = path.to_str().unwrap_or_default().to_string();
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();

        ImageStruct {
            name,
            path: path_str,
            tags,
        }
    }

    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_image_struct_creation() {
        let path = Path::new("/images/photo.jpg");
        let tags = vec!["vacation".to_string(), "summer".to_string()];
        let image = ImageStruct::new(path, tags.clone());

        assert_eq!(image.name, "photo.jpg");
        assert_eq!(image.path, "/images/photo.jpg");
        assert_eq!(image.tags, tags);
    }

    #[test]
    fn test_add_tag() {
        let path = Path::new("/images/photo.jpg");
        let mut image = ImageStruct::new(path, Vec::new());

        image.add_tag("nature".to_string());
        assert_eq!(image.tags, vec!["nature".to_string()]);

        // Adding the same tag should not duplicate it
        image.add_tag("nature".to_string());
        assert_eq!(image.tags, vec!["nature".to_string()]);
    }
}