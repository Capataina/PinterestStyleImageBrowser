//! ALTER TABLE migration helpers for existing databases.
//!
//! `initialize()` (in `mod.rs`) creates every table with `CREATE TABLE
//! IF NOT EXISTS`, so brand-new installs get the latest schema in one
//! shot. These helpers handle the other case: a DB file written by an
//! older build that's missing columns added in subsequent phases.
//! Each helper is gated by a `PRAGMA table_info(images)` check so the
//! ALTER only fires when the column is actually missing — the helpers
//! are idempotent and safe to call on every launch.

use tracing::info;

use super::ImageDatabase;

impl ImageDatabase {
    /// Migrate existing databases to add thumbnail columns if they don't exist
    pub(super) fn migrate_add_thumbnail_columns(&self) -> rusqlite::Result<()> {
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
            info!("Thumbnail-columns migration complete.");
        }

        Ok(())
    }

    /// Add the root_id column to images for multi-folder support. Old
    /// rows get root_id = NULL initially; the lib.rs::run setup
    /// callback handles the per-row backfill once it knows what root
    /// (if any) was previously configured via settings.json.
    pub(super) fn migrate_add_multifolder_columns(&self) -> rusqlite::Result<()> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info(images)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();

        if !columns.contains(&"root_id".to_string()) {
            info!("Migrating database: Adding root_id column for multi-folder...");
            conn.execute(
                "ALTER TABLE images ADD COLUMN root_id INTEGER REFERENCES roots(id) ON DELETE CASCADE",
                [],
            )?;
            info!("Multi-folder migration complete.");
        }

        Ok(())
    }

    /// Add notes (free-text per-image annotations, Phase 11) and
    /// orphaned (deleted-from-disk marker, Phase 7) columns.
    pub(super) fn migrate_add_notes_and_orphaned_columns(&self) -> rusqlite::Result<()> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info(images)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();

        if !columns.contains(&"notes".to_string()) {
            info!("Migrating database: Adding notes column...");
            conn.execute("ALTER TABLE images ADD COLUMN notes TEXT", [])?;
        }
        if !columns.contains(&"orphaned".to_string()) {
            info!("Migrating database: Adding orphaned column...");
            conn.execute(
                "ALTER TABLE images ADD COLUMN orphaned INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        Ok(())
    }
}
