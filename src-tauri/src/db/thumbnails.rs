//! Per-image thumbnail metadata (path on disk + original dimensions).
//!
//! The thumbnail file itself lives outside the DB (managed by the
//! thumbnail generator); the columns here are just the pointer + the
//! width/height we use for the masonry layout.

use super::{ID, ImageDatabase};

impl ImageDatabase {
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
}
