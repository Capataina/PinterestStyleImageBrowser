//! Image insert + per-image notes + orphan-detection lifecycle.
//!
//! Grouped together because all three concerns mutate the `images`
//! table directly outside the read-paths in `images_query.rs`:
//!   * `add_image` is the single insertion point used by the indexing
//!     pipeline.
//!   * `get_image_notes` / `set_image_notes` manage the Phase-11 free
//!     text annotations column.
//!   * `mark_orphaned` is the Phase-7 deleted-from-disk lifecycle —
//!     called by the indexing pipeline's orphan-detection pass.

use std::collections::HashSet;

use rusqlite::{params, params_from_iter};

use super::{ID, ImageDatabase};

impl ImageDatabase {
    /// Set or clear the orphaned flag on every image in a given root.
    /// Used by the indexing pipeline's orphan-detection pass — after a
    /// scan we know exactly which paths exist on disk, and any DB row
    /// for that root whose path isn't in the live set gets marked
    /// orphaned. The grid query filters orphaned rows out so the user
    /// doesn't see deleted images.
    ///
    /// Returns the number of rows updated.
    pub fn mark_orphaned(&self, root_id: ID, alive_paths: &[String]) -> rusqlite::Result<usize> {
        let conn = self.connection.lock().unwrap();

        // Re-mark every row from this root as not-orphaned first.
        // Necessary because a previously-orphaned row whose file came
        // back (rename, restore from trash) should re-appear in the grid.
        conn.execute(
            "UPDATE images SET orphaned = 0 WHERE root_id = ?1",
            [root_id],
        )?;

        if alive_paths.is_empty() {
            // Edge case: empty scan (e.g. user pointed at a now-empty
            // folder). Mark every row from this root orphaned.
            let n = conn.execute(
                "UPDATE images SET orphaned = 1 WHERE root_id = ?1",
                [root_id],
            )?;
            return Ok(n);
        }

        // Two-pass approach without temp tables: load all paths from the
        // root, diff against the alive set in Rust, then UPDATE the
        // diff. This avoids constructing a multi-thousand-element IN
        // clause that would blow past SQLite's parameter limits on
        // large libraries.
        let mut stmt = conn.prepare("SELECT id, path FROM images WHERE root_id = ?1")?;
        let rows: Vec<(ID, String)> = stmt
            .query_map([root_id], |r| Ok((r.get::<_, ID>(0)?, r.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let alive_set: HashSet<&str> = alive_paths.iter().map(|s| s.as_str()).collect();
        let to_orphan: Vec<ID> = rows
            .iter()
            .filter(|(_, p)| !alive_set.contains(p.as_str()))
            .map(|(id, _)| *id)
            .collect();

        if to_orphan.is_empty() {
            return Ok(0);
        }

        let mut updated = 0;
        for chunk in to_orphan.chunks(500) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "UPDATE images SET orphaned = 1 WHERE id IN ({placeholders})"
            );
            updated += conn.execute(&sql, params_from_iter(chunk))?;
        }
        Ok(updated)
    }

    /// Insert an image path. With multi-folder support each row remembers
    /// which root it came from. Idempotent via `INSERT OR IGNORE` on the
    /// path uniqueness constraint — a re-scan never duplicates rows.
    pub fn add_image(&self, path: String, root_id: Option<ID>) -> rusqlite::Result<()> {
        let conn = self.connection.lock().unwrap();
        match root_id {
            Some(rid) => {
                conn.execute(
                    "INSERT OR IGNORE INTO images (path, root_id) VALUES (?1, ?2)",
                    params![path, rid],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT OR IGNORE INTO images (path) VALUES (?1)",
                    [path],
                )?;
            }
        }
        Ok(())
    }

    /// Read the free-text annotation for an image. Returns Ok(None)
    /// when the row exists but the column is NULL (default).
    pub fn get_image_notes(&self, image_id: ID) -> rusqlite::Result<Option<String>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT notes FROM images WHERE id = ?1")?;
        let mut rows = stmt.query([image_id])?;
        match rows.next()? {
            Some(row) => row.get::<_, Option<String>>(0),
            None => Err(rusqlite::Error::QueryReturnedNoRows),
        }
    }

    /// Set / clear the free-text annotation. Pass an empty string or
    /// "" to clear; we don't bother distinguishing "" from NULL because
    /// the user-facing semantic is the same ("no annotation").
    pub fn set_image_notes(&self, image_id: ID, notes: &str) -> rusqlite::Result<()> {
        let cleaned = notes.trim();
        let val: Option<&str> = if cleaned.is_empty() { None } else { Some(cleaned) };
        self.connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE images SET notes = ?1 WHERE id = ?2",
                params![val, image_id],
            )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::fresh_db;

    // ============================================================
    //  Phase 7: orphan detection
    // ============================================================

    #[test]
    fn mark_orphaned_marks_missing_paths() {
        let db = fresh_db();
        let r = db.add_root("/r".into()).unwrap();
        db.add_image("/r/keep.jpg".into(), Some(r.id)).unwrap();
        db.add_image("/r/lost.jpg".into(), Some(r.id)).unwrap();

        // Only "keep" is in the alive set.
        let alive = vec!["/r/keep.jpg".to_string()];
        let n = db.mark_orphaned(r.id, &alive).unwrap();
        assert_eq!(n, 1, "exactly one image should have been orphaned");

        let imgs = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert_eq!(imgs.len(), 1, "orphaned row should be filtered out");
        assert_eq!(imgs[0].path, "/r/keep.jpg");
    }

    #[test]
    fn mark_orphaned_unmarks_returned_files() {
        let db = fresh_db();
        let r = db.add_root("/r".into()).unwrap();
        db.add_image("/r/file.jpg".into(), Some(r.id)).unwrap();
        // First scan: file is alive.
        db.mark_orphaned(r.id, &["/r/file.jpg".into()]).unwrap();
        // Second scan: file disappeared.
        db.mark_orphaned(r.id, &[]).unwrap();
        let visible = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert!(visible.is_empty());
        // Third scan: file returned.
        db.mark_orphaned(r.id, &["/r/file.jpg".into()]).unwrap();
        let visible = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert_eq!(visible.len(), 1);
    }

    #[test]
    fn mark_orphaned_empty_alive_set_orphans_everything_in_root() {
        let db = fresh_db();
        let r = db.add_root("/r".into()).unwrap();
        for i in 0..3 {
            db.add_image(format!("/r/{i}.jpg"), Some(r.id)).unwrap();
        }
        let n = db.mark_orphaned(r.id, &[]).unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn mark_orphaned_does_not_affect_other_roots() {
        let db = fresh_db();
        let a = db.add_root("/a".into()).unwrap();
        let b = db.add_root("/b".into()).unwrap();
        db.add_image("/a/1.jpg".into(), Some(a.id)).unwrap();
        db.add_image("/b/1.jpg".into(), Some(b.id)).unwrap();

        // Empty alive set for root a should orphan a's images, not b's.
        db.mark_orphaned(a.id, &[]).unwrap();
        let visible = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].path, "/b/1.jpg");
    }

    #[test]
    fn mark_orphaned_chunks_handle_large_libraries() {
        // The chunked-IN logic kicks in above 500 ids. Stress with 1200
        // to exercise the chunk boundary.
        let db = fresh_db();
        let r = db.add_root("/big".into()).unwrap();
        for i in 0..1200 {
            db.add_image(format!("/big/{i}.jpg"), Some(r.id)).unwrap();
        }
        // Empty alive set => all 1200 orphan.
        let n = db.mark_orphaned(r.id, &[]).unwrap();
        assert_eq!(n, 1200);
    }

    // ============================================================
    //  Phase 11: notes
    // ============================================================

    #[test]
    fn notes_round_trip() {
        let db = fresh_db();
        db.add_image("/img.jpg".into(), None).unwrap();
        let id = db.get_image_id_by_path("/img.jpg").unwrap();
        // Initially NULL.
        assert_eq!(db.get_image_notes(id).unwrap(), None);

        db.set_image_notes(id, "a personal note").unwrap();
        assert_eq!(
            db.get_image_notes(id).unwrap(),
            Some("a personal note".to_string())
        );

        // Setting empty / whitespace should clear the field.
        db.set_image_notes(id, "   ").unwrap();
        assert_eq!(db.get_image_notes(id).unwrap(), None);
    }

    #[test]
    fn notes_get_returns_none_when_unset() {
        let db = fresh_db();
        db.add_image("/img.jpg".into(), None).unwrap();
        let id = db.get_image_id_by_path("/img.jpg").unwrap();
        assert!(db.get_image_notes(id).unwrap().is_none());
    }

    #[test]
    fn notes_persist_across_reads() {
        let db = fresh_db();
        db.add_image("/img.jpg".into(), None).unwrap();
        let id = db.get_image_id_by_path("/img.jpg").unwrap();
        db.set_image_notes(id, "first").unwrap();
        // Second read should still see the value.
        for _ in 0..5 {
            assert_eq!(
                db.get_image_notes(id).unwrap(),
                Some("first".to_string())
            );
        }
    }
}
