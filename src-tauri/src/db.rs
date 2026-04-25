use std::{collections::HashMap, sync::Mutex};

use rusqlite::{fallible_iterator::FallibleIterator, params_from_iter};
use tracing::info;

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
        // Create images table with all columns including thumbnail support
        self.connection.lock().unwrap().execute(
            "
            CREATE TABLE IF NOT EXISTS images (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                embedding BLOB,
                thumbnail_path TEXT,
                width INTEGER,
                height INTEGER
            );",
            [],
        )?;

        // Migration: Add thumbnail columns if they don't exist (for existing databases)
        self.migrate_add_thumbnail_columns()?;

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

    /// Migrate existing databases to add thumbnail columns if they don't exist
    fn migrate_add_thumbnail_columns(&self) -> rusqlite::Result<()> {
        let conn = self.connection.lock().unwrap();

        // Check if thumbnail_path column exists
        let mut stmt = conn.prepare("PRAGMA table_info(images)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();

        if !columns.contains(&"thumbnail_path".to_string()) {
            info!("Migrating database: Adding thumbnail columns...");
            conn.execute("ALTER TABLE images ADD COLUMN thumbnail_path TEXT", [])?;
            conn.execute("ALTER TABLE images ADD COLUMN width INTEGER", [])?;
            conn.execute("ALTER TABLE images ADD COLUMN height INTEGER", [])?;
            info!("Migration complete.");
        }

        Ok(())
    }

    /// Default path to the SQLite database file in the platform-correct
    /// app data directory (e.g. `~/Library/Application Support/com.ataca.image-browser/images.db`
    /// on macOS). Created on first launch.
    pub fn default_database_path() -> String {
        crate::paths::database_path()
            .to_string_lossy()
            .into_owned()
    }

    pub fn add_image(&self, path: String) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("INSERT OR IGNORE INTO images (path) VALUES (?1)", [path])?;
        Ok(())
    }

    /// Clear every image and image-tag row, leaving the schema intact and
    /// preserving the user's tag catalogue. Used when the user picks a new
    /// scan root — the single-root replaceable model means orphan rows from
    /// the previous root must go.
    ///
    /// `images_tags` clears via the `ON DELETE CASCADE` on the FK from the
    /// images delete; we still issue the explicit DELETE first as a belt-
    /// and-braces measure in case a future schema change drops the cascade.
    pub fn wipe_images_for_new_root(&self) -> rusqlite::Result<()> {
        let conn = self.connection.lock().unwrap();
        conn.execute("DELETE FROM images_tags", [])?;
        conn.execute("DELETE FROM images", [])?;
        Ok(())
    }

    pub fn create_tag(&self, name: String, color: String) -> rusqlite::Result<Tag> {
        let conn = self.connection.lock().unwrap();
        conn.execute(
            "INSERT INTO tags (name, color) VALUES (?1, ?2)",
            [name.clone(), color.clone()],
        )?;
        return Ok(Tag::new(conn.last_insert_rowid(), name, color));
    }

    pub fn delete_tag(&self, tag_id: ID) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        Ok(())
    }

    pub fn remove_tag_from_image(&self, image_id: ID, tag_id: ID) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM images_tags WHERE image_id = ?1 AND tag_id = ?2",
            [image_id, tag_id],
        )?;
        Ok(())
    }

    pub fn add_tag_to_image(&self, image_id: ID, tag_id: ID) -> rusqlite::Result<()> {
        // INSERT OR IGNORE so duplicate (image_id, tag_id) assignments are
        // a no-op rather than a UNIQUE-constraint error. The frontend
        // pre-checks selection state, but a future caller that doesn't
        // shouldn't have to.
        self.connection.lock().unwrap().execute(
            "INSERT OR IGNORE INTO images_tags (image_id, tag_id) VALUES (?1, ?2)",
            [image_id, tag_id],
        )?;
        Ok(())
    }

    pub fn get_tags(&self) -> rusqlite::Result<Vec<Tag>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM tags ORDER BY id;")?;

        let rows = stmt.query([])?;

        return rows
            .map(|r| Ok(Tag::new(r.get("id")?, r.get("name")?, r.get("color")?)))
            .collect();
    }

    pub fn get_images(
        &self,
        filter_tag_ids: Vec<ID>,
        _filter_string: String,
    ) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();

        let sql = if filter_tag_ids.len() > 0 {
            let placeholders = vec!["?"; filter_tag_ids.len()].join(", ");
            format!(
                "SELECT images.id AS img_id, images.path AS img_path,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id
            WHERE EXISTS (
                SELECT 1
                FROM images_tags it2
                WHERE it2.image_id = images.id
                AND it2.tag_id IN ({})
            );",
                placeholders
            )
        } else {
            "SELECT images.id AS img_id, images.path AS img_path,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id;"
                .to_string()
        };
        let mut stmt = conn.prepare(&sql)?;

        let mut rows = stmt.query(params_from_iter(filter_tag_ids))?;
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

        let mut images: Vec<ImageData> = map
            .into_iter()
            .map(|(id, (path, tags))| ImageData::new(id, std::path::Path::new(&path), tags))
            .collect();
        images.sort_by_key(|img| img.id);

        Ok(images)
    }

    pub fn get_all_images(&self) -> rusqlite::Result<Vec<ImageData>> {
        self.get_images(Vec::new(), "".to_string())
    }

    // Get images that don't have embeddings yet
    pub fn get_images_without_embeddings(&self) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT images.id AS img_id, images.path AS img_path,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id
            WHERE images.embedding IS NULL;",
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

        let mut images: Vec<ImageData> = map
            .into_iter()
            .map(|(id, (path, tags))| ImageData::new(id, std::path::Path::new(&path), tags))
            .collect();
        images.sort_by_key(|img| img.id);

        Ok(images)
    }

    // update the embedding of an image
    pub fn update_image_embedding(
        &self,
        image_id: ID,
        embedding: Vec<f32>,
    ) -> rusqlite::Result<()> {
        // Handle empty embeddings explicitly
        if embedding.is_empty() {
            self.connection.lock().unwrap().execute(
                "UPDATE images SET embedding = ?1 WHERE id = ?2",
                rusqlite::params![&[] as &[u8], image_id],
            )?;
            return Ok(());
        }

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
        if let Some(row) = rows.next()? {
            // Handle NULL embeddings
            let embedding_bytes: Option<Vec<u8>> = row.get("embedding")?;

            match embedding_bytes {
                None => Err(rusqlite::Error::QueryReturnedNoRows),
                Some(bytes) => {
                    // Handle empty embeddings
                    if bytes.is_empty() {
                        return Ok(Vec::new());
                    }

                    // Ensure the byte length is a multiple of f32 size
                    let f32_size = std::mem::size_of::<f32>();
                    if bytes.len() % f32_size != 0 {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Blob,
                            format!(
                                "Embedding byte length {} is not a multiple of f32 size ({})",
                                bytes.len(),
                                f32_size
                            )
                            .into(),
                        ));
                    }

                    // Convert bytes back to Vec<f32>
                    let embedding: Vec<f32> = unsafe {
                        std::slice::from_raw_parts(
                            bytes.as_ptr() as *const f32,
                            bytes.len() / f32_size,
                        )
                        .to_vec()
                    };
                    Ok(embedding)
                }
            }
        } else {
            Err(rusqlite::Error::QueryReturnedNoRows)
        }
    }

    /// Fetch every (id, path, embedding) row whose embedding is non-null
    /// in a single SELECT.
    ///
    /// Replaces the per-row `get_image_embedding(id)` call inside the
    /// cosine populate loop, which was N+1 (one query per image —
    /// ~30x slower than this for 1000+ image libraries). The path is
    /// returned alongside the embedding because that's what the cosine
    /// index keys by.
    ///
    /// Empty embeddings are skipped at the SQL level (`length(embedding) > 0`)
    /// so callers don't have to filter them out.
    pub fn get_all_embeddings(&self) -> rusqlite::Result<Vec<(ID, String, Vec<f32>)>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, embedding FROM images
             WHERE embedding IS NOT NULL AND length(embedding) > 0",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: ID = row.get(0)?;
            let path: String = row.get(1)?;
            let bytes: Vec<u8> = row.get(2)?;
            Ok((id, path, bytes))
        })?;

        let mut out = Vec::new();
        let f32_size = std::mem::size_of::<f32>();
        for row in rows {
            let (id, path, bytes) = row?;
            if bytes.len() % f32_size != 0 {
                // Skip malformed rows — they were probably written by an
                // older version with a different layout. The user can
                // wipe and re-encode.
                continue;
            }
            let embedding: Vec<f32> = unsafe {
                std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / f32_size)
                    .to_vec()
            };
            out.push((id, path, embedding));
        }
        Ok(out)
    }

    pub fn get_image_id_by_path(&self, path: &str) -> rusqlite::Result<ID> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM images WHERE path = ?1 LIMIT 1")?;
        let mut rows = stmt.query([path])?;
        if let Some(row) = rows.next()? {
            Ok(row.get("id")?)
        } else {
            Err(rusqlite::Error::QueryReturnedNoRows)
        }
    }

    // ==================== Thumbnail Methods ====================

    /// Get images that don't have thumbnails yet
    pub fn get_images_without_thumbnails(&self) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT images.id AS img_id, images.path AS img_path,
            images.thumbnail_path, images.width, images.height,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id
            WHERE images.thumbnail_path IS NULL OR images.thumbnail_path = '';",
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

        let mut images: Vec<ImageData> = map
            .into_iter()
            .map(|(id, (path, tags))| ImageData::new(id, std::path::Path::new(&path), tags))
            .collect();
        images.sort_by_key(|img| img.id);

        Ok(images)
    }

    /// Update thumbnail path and original dimensions for an image
    pub fn update_image_thumbnail(
        &self,
        image_id: ID,
        thumbnail_path: &std::path::Path,
        width: u32,
        height: u32,
    ) -> rusqlite::Result<()> {
        let thumbnail_path_str = thumbnail_path.to_string_lossy().to_string();
        self.connection.lock().unwrap().execute(
            "UPDATE images SET thumbnail_path = ?1, width = ?2, height = ?3 WHERE id = ?4",
            rusqlite::params![thumbnail_path_str, width as i64, height as i64, image_id],
        )?;
        Ok(())
    }

    /// Get thumbnail info for an image (thumbnail_path, width, height)
    pub fn get_image_thumbnail_info(
        &self,
        image_id: ID,
    ) -> rusqlite::Result<Option<(String, u32, u32)>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT thumbnail_path, width, height FROM images WHERE id = ?1")?;

        let mut rows = stmt.query([image_id])?;
        if let Some(row) = rows.next()? {
            let thumbnail_path: Option<String> = row.get(0)?;
            let width: Option<i64> = row.get(1)?;
            let height: Option<i64> = row.get(2)?;

            if let (Some(path), Some(w), Some(h)) = (thumbnail_path, width, height) {
                if !path.is_empty() {
                    return Ok(Some((path, w as u32, h as u32)));
                }
            }
        }

        Ok(None)
    }

    /// Get images with their thumbnail info included
    pub fn get_images_with_thumbnails(
        &self,
        filter_tag_ids: Vec<ID>,
        _filter_string: String,
    ) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();

        let sql = if filter_tag_ids.len() > 0 {
            let placeholders = vec!["?"; filter_tag_ids.len()].join(", ");
            format!(
                "SELECT images.id AS img_id, images.path AS img_path,
                images.thumbnail_path, images.width, images.height,
                tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
                FROM images
                LEFT JOIN images_tags ON images.id = images_tags.image_id
                LEFT JOIN tags ON tags.id = images_tags.tag_id
                WHERE EXISTS (
                    SELECT 1
                    FROM images_tags it2
                    WHERE it2.image_id = images.id
                    AND it2.tag_id IN ({})
                );",
                placeholders
            )
        } else {
            "SELECT images.id AS img_id, images.path AS img_path,
            images.thumbnail_path, images.width, images.height,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id;"
                .to_string()
        };
        let mut stmt = conn.prepare(&sql)?;

        let mut rows = stmt.query(params_from_iter(filter_tag_ids))?;

        // Map: image_id -> (path, tags, thumbnail_path, width, height)
        let mut map: HashMap<ID, (String, Vec<Tag>, Option<String>, Option<i64>, Option<i64>)> =
            HashMap::new();

        // aggregate tags and thumbnail info
        while let Some(row) = rows.next()? {
            let img_id: ID = row.get("img_id")?;
            let img_path: String = row.get("img_path")?;
            let thumbnail_path: Option<String> = row.get("thumbnail_path")?;
            let width: Option<i64> = row.get("width")?;
            let height: Option<i64> = row.get("height")?;
            let tag_id_opt: Option<ID> = row.get("tag_id")?;

            let entry =
                map.entry(img_id)
                    .or_insert((img_path, Vec::new(), thumbnail_path, width, height));
            if let Some(tag_id) = tag_id_opt {
                let tag = Tag {
                    id: tag_id,
                    name: row.get("tag_name")?,
                    color: row.get("tag_color")?,
                };
                entry.1.push(tag);
            }
        }

        let mut images: Vec<ImageData> = map
            .into_iter()
            .map(|(id, (path, tags, thumbnail_path, width, height))| {
                let mut img = ImageData::new(id, std::path::Path::new(&path), tags);
                img.thumbnail_path = thumbnail_path;
                img.width = width.map(|w| w as u32);
                img.height = height.map(|h| h as u32);
                img
            })
            .collect();

        // Shuffle images randomly instead of sorting by ID
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        images.shuffle(&mut rng);

        Ok(images)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_operations() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();

        let images = db.get_images(vec![], "".to_string()).unwrap();
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn test_prevent_duplicate_images() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();
        db.add_image(test_image_path.to_owned()).unwrap(); // Attempt to add duplicate

        let images = db.get_images(vec![], "".to_string()).unwrap();
        assert_eq!(images.len(), 1); // Should still be only one image
    }

    #[test]
    fn test_empty_database() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let images = db.get_images(vec![], "".to_string()).unwrap();
        assert_eq!(images.len(), 0); // No images should be present
    }

    #[test]
    fn test_update_image_embedding_basic() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        // Add an image first
        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();

        // Get the image ID
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Create a test embedding (512 dimensions, typical for CLIP)
        let test_embedding: Vec<f32> = (0..512).map(|i| i as f32 * 0.001).collect();

        // Update the embedding
        db.update_image_embedding(image_id, test_embedding.clone())
            .expect("Failed to update embedding");

        // Verify the embedding was stored correctly
        let retrieved_embedding = db
            .get_image_embedding(image_id)
            .expect("Failed to retrieve embedding");

        assert_eq!(
            retrieved_embedding.len(),
            test_embedding.len(),
            "Embedding length mismatch"
        );

        // Verify the values match (with small tolerance for floating point)
        for (retrieved, original) in retrieved_embedding.iter().zip(test_embedding.iter()) {
            assert!(
                (retrieved - original).abs() < 1e-6,
                "Embedding value mismatch: {} vs {}",
                retrieved,
                original
            );
        }
    }

    #[test]
    fn test_update_image_embedding_round_trip() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/test_image.png";
        db.add_image(test_image_path.to_owned()).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Create a realistic embedding (normalized vector)
        let mut embedding: Vec<f32> = (0..512).map(|i| (i as f32).sin()).collect();
        // Normalize the embedding
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        embedding.iter_mut().for_each(|x| *x /= norm);

        // Store it
        db.update_image_embedding(image_id, embedding.clone())
            .expect("Failed to store embedding");

        // Retrieve it
        let retrieved = db
            .get_image_embedding(image_id)
            .expect("Failed to retrieve embedding");

        // Verify dimensions
        assert_eq!(retrieved.len(), 512, "Embedding should be 512 dimensions");

        // Verify values match
        for (i, (ret, orig)) in retrieved.iter().zip(embedding.iter()).enumerate() {
            assert!(
                (ret - orig).abs() < 1e-5,
                "Value mismatch at index {}: {} vs {}",
                i,
                ret,
                orig
            );
        }
    }

    #[test]
    fn test_update_image_embedding_overwrite() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Store first embedding
        let first_embedding: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        db.update_image_embedding(image_id, first_embedding.clone())
            .expect("Failed to store first embedding");

        // Verify first embedding is stored
        let retrieved1 = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved1, first_embedding);

        // Store second embedding (different size)
        let second_embedding: Vec<f32> = vec![5.0, 6.0, 7.0, 8.0, 9.0];
        db.update_image_embedding(image_id, second_embedding.clone())
            .expect("Failed to overwrite embedding");

        // Verify second embedding replaced the first
        let retrieved2 = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved2, second_embedding);
        assert_ne!(retrieved2, first_embedding);
    }

    #[test]
    fn test_update_image_embedding_nonexistent_image() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        // Try to update embedding for non-existent image
        let fake_image_id = 99999;
        let test_embedding: Vec<f32> = vec![1.0, 2.0, 3.0];

        // This should succeed (UPDATE doesn't fail if no rows match, it just updates 0 rows)
        // But we should verify the embedding wasn't actually stored
        let result = db.update_image_embedding(fake_image_id, test_embedding.clone());

        // The update itself should succeed (SQL UPDATE doesn't error on no matches)
        assert!(
            result.is_ok(),
            "UPDATE should succeed even if no rows match"
        );

        // But retrieving should fail
        let retrieve_result = db.get_image_embedding(fake_image_id);
        assert!(
            retrieve_result.is_err(),
            "Should fail to retrieve embedding for non-existent image"
        );
    }

    #[test]
    fn test_update_image_embedding_empty_embedding() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Store empty embedding
        let empty_embedding: Vec<f32> = Vec::new();
        db.update_image_embedding(image_id, empty_embedding.clone())
            .expect("Failed to store empty embedding");

        // Retrieve and verify
        let retrieved = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved.len(), 0);
        assert_eq!(retrieved, empty_embedding);
    }

    #[test]
    fn test_update_image_embedding_large_embedding() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Create a large embedding (larger than typical 512)
        let large_embedding: Vec<f32> = (0..2048).map(|i| i as f32).collect();

        db.update_image_embedding(image_id, large_embedding.clone())
            .expect("Failed to store large embedding");

        let retrieved = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved.len(), 2048);
        assert_eq!(retrieved, large_embedding);
    }

    #[test]
    fn test_get_image_embedding_before_update() {
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned()).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Try to get embedding before it's set (should be NULL in DB)
        let result = db.get_image_embedding(image_id);

        // This should fail because the embedding is NULL
        assert!(result.is_err(), "Should fail to retrieve NULL embedding");
    }
}
