use std::{collections::HashMap, sync::Mutex};

use crate::{image_struct::ImageData, tag_struct::Tag};

pub struct ImageDatabase {
    connection: Mutex<rusqlite::Connection>,
}

pub type ID = i64;

impl ImageDatabase {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let connection = rusqlite::Connection::open(db_path)?;
        Ok(ImageDatabase {
            connection: Mutex::new(connection),
        })
    }

    pub fn initialize(&self) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "
            CREATE TABLE IF NOT EXISTS images (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                embedding BLOB
            );",
            [],
        )?;

        self.connection.lock().unwrap().execute(
            "CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL
            );",
            [],
        )?;

        self.connection.lock().unwrap().execute(
            "CREATE TABLE IF NOT EXISTS images_tags (
                image_id INTEGER NOT NULL,
                tag_id INTEGER NOT NULL,
                PRIMARY KEY (image_id, tag_id),
                FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            );",
            [],
        )?;
        Ok(())
    }

    pub fn default_database_path() -> String {
        let default_path = "./images.db".to_string();
        default_path
    }

    pub fn add_image(&mut self, path: String) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("INSERT OR IGNORE INTO images (path) VALUES (?1)", [path])?;
        Ok(())
    }

    pub fn create_tag(&mut self, name: String, color: String) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT OR IGNORE INTO tags (name, color) VALUES (?1, ?2)",
            [name, color],
        )?;
        Ok(())
    }

    pub fn delete_tag(&mut self, tag_id: ID) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        Ok(())
    }

    pub fn remove_tag_from_image(&mut self, image_id: ID, tag_id: ID) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM images_tags WHERE image_id = ?1 AND tag_id = ?2",
            [image_id, tag_id],
        )?;
        Ok(())
    }

    pub fn add_tag_to_image(&mut self, image_id: ID, tag_id: ID) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT OR IGNORE INTO images_tags (image_id, tag_id) VALUES (?1, ?2)",
            [image_id, tag_id],
        )?;
        Ok(())
    }

    pub fn get_all_images(&self) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT images.id AS img_id, images.path AS img_path, 
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id;",
        )?;

        let mut rows = stmt.query([])?;
        let mut map: HashMap<ID, (String, Vec<Tag>)> = HashMap::new();

        // aggregate tags
        while let Some(row) = rows.next()? {
            let img_id: ID = row.get("img_id")?;
            let img_path: String = row.get("img_path")?;
            let tag_id_opt: Option<ID> = row.get("tag_id")?;

            let entry = map.entry(img_id).or_insert((img_path, Vec::new()));
            if let Some(tag_id) = tag_id_opt {
                let tag = Tag {
                    id: tag_id,
                    name: row.get("tag_name")?,
                    color: row.get("tag_color")?,
                };
                entry.1.push(tag);
            }
        }

        let images = map
            .into_iter()
            .map(|(id, (path, tags))| ImageData::new(id, std::path::Path::new(&path), tags))
            .collect();

        Ok(images)
    }

    // update the embedding of an image
    pub fn update_image_embedding(
        &mut self,
        image_id: ID,
        embedding: Vec<f32>,
    ) -> rusqlite::Result<()> {
        // Convert Vec<f32> to bytes for BLOB storage
        let embedding_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                embedding.as_ptr() as *const u8,
                embedding.len() * std::mem::size_of::<f32>(),
            )
        };
        self.connection.lock().unwrap().execute(
            "UPDATE images SET embedding = ?1 WHERE id = ?2",
            rusqlite::params![embedding_bytes, image_id],
        )?;
        Ok(())
    }

    // function to get the embedding of an image
    pub fn get_image_embedding(&self, image_id: ID) -> rusqlite::Result<Vec<f32>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT embedding FROM images WHERE id = ?1")?;
        let mut rows = stmt.query([image_id])?;
        let row = rows.next()?;
        let embedding: Vec<f32> = row.get("embedding")?;
        Ok(embedding)
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
        db.add_image(test_image_path.to_owned()).unwrap();

        let images = db.get_all_images().unwrap();
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn test_prevent_duplicate_images() {
        let mut db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();
        db.add_image(test_image_path.to_owned()).unwrap(); // Attempt to add duplicate

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
