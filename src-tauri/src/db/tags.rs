//! Tag catalogue + image↔tag association table.
//!
//! Tags themselves live in the `tags` table; the many-to-many link to
//! images lives in `images_tags`. Queries that JOIN those two for the
//! grid live in `images_query.rs`; the methods here are the small
//! mutation surface for managing the catalogue.

use rusqlite::fallible_iterator::FallibleIterator;

use super::{ID, ImageDatabase};
use crate::tag_struct::Tag;

impl ImageDatabase {
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
}
