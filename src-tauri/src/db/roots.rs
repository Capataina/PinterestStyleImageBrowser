//! Roots CRUD + multi-folder lifecycle.
//!
//! A "root" is a folder the user has added to the library. The images
//! table's `root_id` FK points back here so the grid query can filter
//! by enabled roots, and `ON DELETE CASCADE` propagates root removal
//! into the images it owns.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use tracing::info;

use super::{ID, ImageDatabase};
use crate::root_struct::Root;

impl ImageDatabase {
    /// List every configured root, ordered by add date (oldest first).
    pub fn list_roots(&self) -> rusqlite::Result<Vec<Root>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, enabled, added_at FROM roots ORDER BY added_at ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Root {
                id: r.get(0)?,
                path: r.get(1)?,
                enabled: r.get::<_, i64>(2)? != 0,
                added_at: r.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }

    /// Insert a new root. Returns the populated Root row. The path
    /// uniqueness constraint surfaces as an `Err` to the caller when
    /// the user adds the same path twice.
    pub fn add_root(&self, path: String) -> rusqlite::Result<Root> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.connection.lock().unwrap();
        conn.execute(
            "INSERT INTO roots (path, enabled, added_at) VALUES (?1, 1, ?2)",
            params![path, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Root::new(id, path, true, now))
    }

    /// Remove a root. The ON DELETE CASCADE on images.root_id wipes
    /// every image that came from this root.
    pub fn remove_root(&self, id: ID) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM roots WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Toggle a root's enabled flag. Disabled roots keep their image
    /// rows on disk (re-enabling is instant — no re-index) but the
    /// grid filter excludes them.
    pub fn set_root_enabled(&self, id: ID, enabled: bool) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE roots SET enabled = ?1 WHERE id = ?2",
            params![enabled as i64, id],
        )?;
        Ok(())
    }

    /// One-shot migration helper — used by the lib.rs setup callback
    /// when an old single-root setup (settings.json::scan_root) needs to
    /// be folded into the new roots table. Returns the new Root, or
    /// None if a root with that path already exists. Also backfills any
    /// images.root_id NULLs that fall under this path.
    pub fn migrate_legacy_scan_root(&self, path: String) -> rusqlite::Result<Option<Root>> {
        // Idempotent: if a row already exists, leave it alone.
        let conn = self.connection.lock().unwrap();
        let existing: rusqlite::Result<i64> = conn.query_row(
            "SELECT id FROM roots WHERE path = ?1",
            [&path],
            |r| r.get(0),
        );
        if existing.is_ok() {
            return Ok(None);
        }
        drop(conn);

        let root = self.add_root(path.clone())?;

        // Backfill: every NULL-root_id row whose path starts with this
        // root path now belongs to this root.
        let conn = self.connection.lock().unwrap();
        let prefix_pattern = format!("{}%", path);
        let updated = conn.execute(
            "UPDATE images SET root_id = ?1
             WHERE root_id IS NULL AND path LIKE ?2",
            params![root.id, prefix_pattern],
        )?;
        info!("legacy scan_root migration: backfilled {} image rows", updated);
        Ok(Some(root))
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

    /// Look up the root_id for an image given its path. Returns None
    /// when the path isn't in the DB or when the row's root_id is NULL
    /// (legacy un-migrated rows). Used by the thumbnail generator to
    /// route output into the correct per-root subfolder.
    ///
    /// Prefer `get_paths_to_root_ids` when looking up many paths at
    /// once (e.g., the indexing pipeline) — it's one SELECT versus
    /// N. This single-path variant remains for incremental lookups.
    pub fn get_root_id_by_path(&self, path: &str) -> Option<ID> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT root_id FROM images WHERE path = ?1 LIMIT 1")
            .ok()?;
        let mut rows = stmt.query([path]).ok()?;
        if let Ok(Some(row)) = rows.next() {
            row.get::<_, Option<ID>>(0).ok().flatten()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::fresh_db;
    

    #[test]
    fn add_root_creates_row_with_enabled_true() {
        let db = fresh_db();
        let r = db.add_root("/tmp/photos".into()).unwrap();
        assert_eq!(r.path, "/tmp/photos");
        assert!(r.enabled);
        assert!(r.added_at > 0);
    }

    #[test]
    fn add_root_rejects_duplicate_path() {
        let db = fresh_db();
        db.add_root("/tmp/photos".into()).unwrap();
        // Path UNIQUE constraint should error on second insert.
        let result = db.add_root("/tmp/photos".into());
        assert!(
            result.is_err(),
            "second add_root with the same path must error"
        );
    }

    #[test]
    fn list_roots_orders_by_added_at_ascending() {
        let db = fresh_db();
        let a = db.add_root("/a".into()).unwrap();
        // Sleep 1s so added_at differs (unix-second granularity).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = db.add_root("/b".into()).unwrap();
        let listed = db.list_roots().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, a.id);
        assert_eq!(listed[1].id, b.id);
    }

    #[test]
    fn remove_root_cascades_to_images() {
        let db = fresh_db();
        let r = db.add_root("/x".into()).unwrap();
        db.add_image("/x/a.jpg".into(), Some(r.id)).unwrap();
        db.add_image("/x/b.jpg".into(), Some(r.id)).unwrap();
        // Sanity: rows are there
        let imgs = db.get_all_images().unwrap();
        assert_eq!(imgs.len(), 2);

        db.remove_root(r.id).unwrap();
        let after = db.get_all_images().unwrap();
        assert_eq!(after.len(), 0, "CASCADE should have wiped image rows");
        let roots = db.list_roots().unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn remove_root_does_not_affect_other_roots_images() {
        let db = fresh_db();
        let a = db.add_root("/a".into()).unwrap();
        let b = db.add_root("/b".into()).unwrap();
        db.add_image("/a/x.jpg".into(), Some(a.id)).unwrap();
        db.add_image("/b/y.jpg".into(), Some(b.id)).unwrap();
        db.remove_root(a.id).unwrap();
        let after = db.get_all_images().unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].path, "/b/y.jpg");
    }

    #[test]
    fn set_root_enabled_round_trips() {
        let db = fresh_db();
        let r = db.add_root("/r".into()).unwrap();
        assert!(r.enabled);
        db.set_root_enabled(r.id, false).unwrap();
        let listed = db.list_roots().unwrap();
        assert!(!listed[0].enabled);
        db.set_root_enabled(r.id, true).unwrap();
        let listed = db.list_roots().unwrap();
        assert!(listed[0].enabled);
    }

    #[test]
    fn migrate_legacy_scan_root_inserts_and_backfills() {
        let db = fresh_db();
        // Simulate old single-folder state: image rows with NULL root_id
        // whose path falls under a legacy scan_root.
        db.add_image("/legacy/a.jpg".into(), None).unwrap();
        db.add_image("/legacy/sub/b.jpg".into(), None).unwrap();
        // And one image NOT under the legacy root — should NOT be backfilled.
        db.add_image("/elsewhere/c.jpg".into(), None).unwrap();

        let migrated = db.migrate_legacy_scan_root("/legacy".into()).unwrap();
        assert!(migrated.is_some());
        let root = migrated.unwrap();
        assert_eq!(root.path, "/legacy");

        // Backfill verification: the two /legacy rows should now point at
        // the new root, the /elsewhere one should not.
        let conn = db.connection.lock().unwrap();
        let count_for_root: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM images WHERE root_id = ?1",
                [root.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count_for_root, 2);
        let count_orphan: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM images WHERE root_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count_orphan, 1);
    }

    #[test]
    fn migrate_legacy_scan_root_is_idempotent() {
        let db = fresh_db();
        let first = db.migrate_legacy_scan_root("/legacy".into()).unwrap();
        assert!(first.is_some());
        // Second call should detect an existing row and return None
        // rather than create a duplicate or error.
        let second = db.migrate_legacy_scan_root("/legacy".into()).unwrap();
        assert!(second.is_none());
        let roots = db.list_roots().unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn get_root_id_by_path_returns_some_when_known() {
        let db = fresh_db();
        let r = db.add_root("/r".into()).unwrap();
        db.add_image("/r/a.jpg".into(), Some(r.id)).unwrap();
        assert_eq!(db.get_root_id_by_path("/r/a.jpg"), Some(r.id));
    }

    #[test]
    fn get_root_id_by_path_returns_none_when_unknown_or_null() {
        let db = fresh_db();
        // Unknown path
        assert_eq!(db.get_root_id_by_path("/missing.jpg"), None);
        // Known but root_id is NULL
        db.add_image("/null.jpg".into(), None).unwrap();
        assert_eq!(db.get_root_id_by_path("/null.jpg"), None);
    }

    #[test]
    fn wipe_images_for_new_root_preserves_tags() {
        let db = fresh_db();
        db.add_image("/x.jpg".into(), None).unwrap();
        db.create_tag("keepme".into(), "#fff".into()).unwrap();
        db.wipe_images_for_new_root().unwrap();
        let imgs = db.get_all_images().unwrap();
        assert!(imgs.is_empty());
        let tags = db.get_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "keepme");
    }
}
