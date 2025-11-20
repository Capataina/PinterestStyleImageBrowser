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

    pub fn add_image(&mut self, path: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO images (path) VALUES (?1)",
            [path],
        )?;
        Ok(())
    }

    pub fn get_all_images(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.connection.prepare("SELECT path FROM images")?;
        let image_iter = stmt.query_map([], |row| row.get(0))?;

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
        db.add_image(test_image_path).unwrap();

        let images = db.get_all_images().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0], test_image_path);
    }
}