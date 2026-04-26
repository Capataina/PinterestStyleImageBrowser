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

use std::sync::{Mutex, OnceLock};

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

/// SQLite-backed image catalogue.
///
/// Two connections per real on-disk database:
///
/// - **`connection`** — the writer. Every INSERT/UPDATE/DELETE goes
///   through this `Mutex<Connection>`. The encoder pipeline holds it
///   for the duration of each batch transaction (R1 below); foreground
///   IPC writes (tag mutations, root toggles) take it briefly.
/// - **`reader`** — a separate read-only connection on the same file,
///   used by foreground IPC SELECTs (the grid query, semantic search
///   over the cosine cache, etc.). The two connections do not contend
///   on the same Mutex, so a foreground `get_images` call no longer
///   queues behind an in-flight encoder write batch. WAL journal mode
///   makes the underlying reads consistent without taking a SQLite-
///   level lock against the writer.
///
///   `reader` is `None` for `:memory:` databases (tests) — `:memory:`
///   is per-connection storage, so a second connection sees an empty
///   DB. Tests don't have foreground/background contention to worry
///   about, so the read-helper falls back to the writer in that case.
///
/// Performance origin: the on-exit profiling report at perf-1777212369
/// captured two ~22 s `ipc.get_images` outliers. Subspans (Batch 3
/// commit) attribute almost all of that wall time to mutex acquisition,
/// not SQL work — confirming the contention hypothesis. R2 (this) +
/// R1 (encoder INSERT batching) + R3 (PRAGMAs) collapse it together.
pub struct ImageDatabase {
    pub(crate) connection: Mutex<rusqlite::Connection>,
    /// Read-only secondary connection. Set by `initialize()` for real
    /// on-disk databases; remains empty for `:memory:` (tests).
    /// `OnceLock` so `initialize()` can populate it through `&self`
    /// without breaking every existing call site that takes `&self`.
    pub(crate) reader: OnceLock<Mutex<rusqlite::Connection>>,
    /// Stored so `initialize()` can open the reader connection lazily
    /// (after the writer has set WAL mode + created schema).
    db_path: String,
}

impl ImageDatabase {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let connection = rusqlite::Connection::open(db_path)?;
        Ok(ImageDatabase {
            connection: Mutex::new(connection),
            reader: OnceLock::new(),
            db_path: db_path.to_string(),
        })
    }

    /// Foreground-read lock helper. Returns the dedicated read-only
    /// connection's mutex guard if one exists (real on-disk DB);
    /// otherwise falls back to the writer connection (for `:memory:`
    /// tests). Use this from IPC SELECT paths so they don't queue
    /// behind encoder write transactions.
    pub(crate) fn read_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        if let Some(r) = self.reader.get() {
            r.lock().expect("reader mutex poisoned")
        } else {
            self.connection.lock().expect("writer mutex poisoned")
        }
    }

    /// Manual WAL checkpoint — drains the WAL file back into the main
    /// DB. Called from the encoder pipeline between batches so the WAL
    /// doesn't grow unbounded under `wal_autocheckpoint = 0` (set in
    /// `initialize()`). PASSIVE mode does not block readers or writers
    /// — it processes whatever pages are clean and returns; a busy
    /// reader simply means the next checkpoint catches up.
    ///
    /// We disable auto-checkpoint because its default cadence (every
    /// 1000 dirty pages) interleaves with encoder batch commits in a
    /// way that produces multi-second stalls when the auto-checkpoint
    /// happens to fire during an active write transaction. By driving
    /// it from the encoder loop instead, checkpoints land at known
    /// quiet points (between batches) where they cannot block
    /// foreground reads.
    pub fn checkpoint_passive(&self) -> rusqlite::Result<()> {
        // Some(_) when reader exists → real DB, do checkpoint;
        // None → :memory:, no-op (no WAL file).
        if self.reader.get().is_none() {
            return Ok(());
        }
        let conn = self.connection.lock().unwrap();
        conn.pragma_update(None, "wal_checkpoint", "PASSIVE")?;
        Ok(())
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
            // R3 — busy_timeout caps how long a momentary lock contention
            // waits before returning SQLITE_BUSY. 5 s is generous enough
            // that any real-world contention (encoder batch commit while
            // foreground IPC arrives) resolves transparently rather than
            // surfacing as an error to the user. Default of 0 is
            // unhelpful for a multi-connection workload.
            conn.pragma_update(None, "busy_timeout", 5000)?;
            // R3 — disable SQLite's automatic WAL checkpointer. The
            // default (1000 dirty pages) fires unpredictably mid-write
            // batch and was the trigger for the perf-1777212369 22 s
            // stalls. We drive checkpoints ourselves between encoder
            // batches via `checkpoint_passive()` (called from
            // indexing.rs::run_clip_encoder + run_trait_encoder).
            conn.pragma_update(None, "wal_autocheckpoint", 0)?;
            // R3 — cap WAL file size at 64 MiB. Without this it can
            // grow unbounded under bursty writes; the cap forces a
            // truncate at the next quiet checkpoint, keeping disk
            // usage bounded and reducing the cost of fsync at COMMIT.
            conn.pragma_update(None, "journal_size_limit", 67_108_864i64)?;
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

        // R2 — open the read-only secondary connection now that WAL
        // mode + schema are in place on the file. Skip for `:memory:`
        // (a second connection sees a separate empty DB) and for any
        // failure path (we just fall back to the writer in read_lock).
        //
        // SQLITE_OPEN_READ_ONLY guarantees the reader can never write.
        // No need for SQLITE_OPEN_NO_MUTEX — rusqlite already wraps
        // the connection in a Rust Mutex.
        if !self.db_path.starts_with(":memory:")
            && !self.db_path.starts_with("file::memory:")
        {
            match rusqlite::Connection::open_with_flags(
                &self.db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            ) {
                Ok(r) => {
                    // Per-connection PRAGMAs — busy_timeout matters here
                    // because a foreground read may briefly contend with
                    // the writer's COMMIT (writer takes WAL_INDEX exclusive
                    // for the duration of the COMMIT itself; readers wait
                    // microseconds, but `busy_timeout = 0` would surface
                    // that wait as SQLITE_BUSY).
                    let _ = r.pragma_update(None, "busy_timeout", 5000);
                    // Discard the result of set() — if another initialize()
                    // somehow got here first, both connections are valid
                    // readers; using the first-installed one is fine.
                    let _ = self.reader.set(Mutex::new(r));
                }
                Err(e) => {
                    tracing::warn!(
                        "could not open read-only secondary connection ({e}); \
                         foreground SELECTs will share the writer mutex"
                    );
                }
            }
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
