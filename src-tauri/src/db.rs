use crate::image_struct::ImageStruct;

pub struct ImageDatabase {
    connection: rusqlite::Connection,
}

impl ImageDatabase {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let connection = rusqlite::Connection::open(db_path)?;
        Ok(ImageDatabase { connection })
    }

    pub fn initialize(&self) -> rusqlite::Result<()> {
        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS images (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE
            )",
            [],
        )?;
        Ok(())
    }

    pub fn default_database_path() -> String {
        let default_path = "./images.db".to_string();
        default_path
    }

    pub fn add_image(&mut self, image: ImageStruct) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO images (path) VALUES (?1)",
            [image.path],
        )?;
        Ok(())
    }

    pub fn get_all_images(&self) -> rusqlite::Result<Vec<ImageStruct>> {
        let mut stmt = self.connection.prepare("SELECT path FROM images")?;
        let image_iter = stmt.query_map([], |row| {
            let path: String = row.get(0)?;
            Ok(ImageStruct::new(std::path::Path::new(&path), Vec::new()))
        })?;

        let mut images = Vec::new();
        for image in image_iter {
            images.push(image?);
        }
        Ok(images)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_operations() {
        let mut db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        let test_image = ImageStruct::new(std::path::Path::new(test_image_path), Vec::new());
        db.add_image(test_image).unwrap();

        let images = db.get_all_images().unwrap();
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn test_prevent_duplicate_images() {
        let mut db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        let test_image = ImageStruct::new(std::path::Path::new(test_image_path), Vec::new());
        db.add_image(test_image.clone()).unwrap();
        db.add_image(test_image).unwrap(); // Attempt to add duplicate

        let images = db.get_all_images().unwrap();
        assert_eq!(images.len(), 1); // Should still be only one image
    }

    #[test]
    fn test_empty_database() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let images = db.get_all_images().unwrap();
        assert_eq!(images.len(), 0); // No images should be present
    }
}