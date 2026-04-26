//! `ImageDatabase` — SQLite-backed image catalogue.
//!
//! This module was previously a single `db.rs` of ~1.6k lines covering
//! eight distinct concerns. It is now split into focused submodules,
//! each defining its own `impl ImageDatabase { ... }` block. The struct
//! itself, the connection-management plumbing, and the `initialize`
//! flow live here; everything else has moved out.
//!
//! Public API surface is unchanged: every existing call site
//! (`db.add_image(...)`, `db.get_tags()`, etc.) continues to work via
//! the `crate::db::ImageDatabase` path because the inherent-impl blocks
//! in the submodules are merged by the compiler regardless of file.

use std::sync::Mutex;

mod embeddings;
pub mod images_query;
mod notes_orphans;
mod roots;
mod schema_migrations;
mod tags;
mod thumbnails;

#[cfg(test)]
mod test_helpers;

/// Numeric identifier shared by every row type in this DB (images,
/// roots, tags). Always SQLite `INTEGER` (i.e. `i64`).
pub type ID = i64;

/// SQLite-backed image catalogue. Single shared `rusqlite::Connection`
/// guarded by a `Mutex` — the foreground UI thread and the background
/// indexing pipeline both call into this struct. WAL journal mode (set
/// in `initialize`) keeps reads non-blocking under the writer.
pub struct ImageDatabase {
    pub(crate) connection: Mutex<rusqlite::Connection>,
}

impl ImageDatabase {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let connection = rusqlite::Connection::open(db_path)?;
        Ok(ImageDatabase {
            connection: Mutex::new(connection),
        })
    }

    pub fn initialize(&self) -> rusqlite::Result<()> {
        // WAL journal mode + NORMAL synchronous (audit finding).
        //
        // Why WAL: the indexing pipeline opens its own ImageDatabase
        // instance (a second SQLite connection to the same file) so the
        // background indexing thread doesn't block the UI thread on
        // every embedding write. In SQLite's default DELETE journal
        // mode, the writer holds an exclusive lock for the duration of
        // every write transaction, blocking all readers; in WAL mode,
        // readers and the single writer can coexist. SQLite's official
        // recommendation for any multi-connection or write-heavy
        // workload (https://sqlite.org/wal.html).
        //
        // Why NORMAL synchronous: the default FULL fsyncs after every
        // commit — appropriate for a database where torn writes corrupt
        // structure, but unnecessary for this app where every commit is
        // either a tag mutation (user can re-do), a thumbnail update
        // (next launch regenerates), or an embedding write (next launch
        // re-encodes). Power-loss "lose at most the last commit" is
        // recoverable on every code path, and `synchronous = NORMAL` is
        // SQLite's explicitly-recommended pairing for WAL when this
        // trade-off is acceptable.
        //
        // Both PRAGMAs persist for the connection's lifetime; WAL also
        // persists across reopens (it's a property of the DB file).
        // pragma_update is the rusqlite path that returns Result so we
        // surface migration-time failures rather than ignoring them.
        {
            let conn = self.connection.lock().unwrap();
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
        }

        // Foreign-key enforcement is OFF by default in SQLite; turn it
        // on so ON DELETE CASCADE actually fires.
        self.connection
            .lock()
            .unwrap()
            .execute("PRAGMA foreign_keys = ON;", [])?;

        // Roots table — created here so the images table's root_id FK
        // has a target. Multi-folder support (Phase 6); existing single-
        // folder users get migrated below.
        self.connection.lock().unwrap().execute(
            "CREATE TABLE IF NOT EXISTS roots (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                enabled INTEGER NOT NULL DEFAULT 1,
                added_at INTEGER NOT NULL
            );",
            [],
        )?;

        // Images table — `notes` and `orphaned` are Phase 11 / Phase 7
        // additions; `root_id` is Phase 6. Existing DBs migrate via
        // ALTER TABLE below.
        self.connection.lock().unwrap().execute(
            "CREATE TABLE IF NOT EXISTS images (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                embedding BLOB,
                thumbnail_path TEXT,
                width INTEGER,
                height INTEGER,
                root_id INTEGER REFERENCES roots(id) ON DELETE CASCADE,
                notes TEXT,
                orphaned INTEGER NOT NULL DEFAULT 0
            );",
            [],
        )?;

        // Migrations for existing DBs: add thumbnail columns, then
        // multi-folder columns, then notes/orphaned. Each is gated by
        // a PRAGMA table_info check so they're idempotent.
        self.migrate_add_thumbnail_columns()?;
        self.migrate_add_multifolder_columns()?;
        self.migrate_add_notes_and_orphaned_columns()?;

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

        // Per-encoder embeddings table (Phase 2 of the encoder picker
        // work). Lets the project hold multiple embeddings per image —
        // one per encoder model — so the user can search via SigLIP-2,
        // CLIP, DINOv2, etc., all from the same DB.
        //
        // Why a separate table rather than more columns on `images`:
        // adding a new encoder = inserting rows, not migrating schema.
        // Storage cost ~2KB per (image × encoder) — negligible.
        //
        // The legacy `images.embedding` column is preserved for one
        // release cycle; the indexing pipeline now also writes the
        // CLIP embedding to this table with encoder_id="clip_vit_b_32".
        // A future migration can drop the old column once everyone has
        // re-indexed.
        self.connection.lock().unwrap().execute(
            "CREATE TABLE IF NOT EXISTS embeddings (
                image_id INTEGER NOT NULL,
                encoder_id TEXT NOT NULL,
                embedding BLOB NOT NULL,
                PRIMARY KEY (image_id, encoder_id),
                FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE
            );",
            [],
        )?;
        // Helps populate_from_db, which scans by encoder_id, to skip
        // the table-scan step.
        self.connection.lock().unwrap().execute(
            "CREATE INDEX IF NOT EXISTS idx_embeddings_encoder
             ON embeddings(encoder_id);",
            [],
        )?;

        // One-shot embedding-pipeline invalidation. Runs AFTER the
        // embeddings table is created (it issues DELETE against that
        // table). Bumps when CLIP/DINOv2 pipeline changes invalidate
        // prior embeddings.
        self.migrate_embedding_pipeline_version()?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_is_idempotent() {
        // Running initialize() twice should not error or duplicate
        // schema. Real-world callers may call it on every launch.
        let db = ImageDatabase::new(":memory:").unwrap();
        db.initialize().unwrap();
        db.initialize().unwrap();
        // And the basic flow still works.
        db.add_image("/x.jpg".into(), None).unwrap();
        let imgs = db.get_all_images().unwrap();
        assert_eq!(imgs.len(), 1);
    }
}
