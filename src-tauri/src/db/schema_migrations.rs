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

    /// Embedding-pipeline version-bump migration. Runs once when
    /// the version stored in `meta` (key `embedding_pipeline_version`)
    /// is less than the current version. Wipes embeddings produced
    /// by the previous pipeline so the next indexing pass re-encodes
    /// everything with the new model + preprocessing.
    ///
    /// Why this is needed:
    /// - **CLIP**: switched from the combined-graph multilingual model
    ///   (in a different embedding space than the image branch) to
    ///   the SEPARATE OpenAI vision_model.onnx + text_model.onnx.
    ///   New preprocessing (bicubic-shortest-edge-224 + center-crop +
    ///   L2-normalize) produces a different embedding distribution
    ///   than the old (resize_exact-224 + Lanczos3 + no-normalize).
    ///   Mixing the two corrupts cosine similarity rankings.
    /// - **DINOv2**: switched from `dinov2_small` (384-d, wrong
    ///   preprocessing) to `dinov2_base` (768-d, correct preprocessing).
    ///   Old `dinov2_small` rows are abandoned because the encoder_id
    ///   changed; this just cleans them up to free disk.
    ///
    /// Bump `CURRENT_PIPELINE_VERSION` whenever a future change
    /// invalidates existing embeddings.
    pub(super) fn migrate_embedding_pipeline_version(&self) -> rusqlite::Result<()> {
        // Version 3 — bumped 2026-04-26 with the Tier-1 + Tier-2 perf
        // bundle. The R6 fast_image_resize swap changes the resize
        // backend (image-rs CatmullRom → fast_image_resize Lanczos3);
        // R7 changes the JPEG decode path (full decode → scaled IDCT
        // for JPEGs). Both produce subtly different RGB buffers than
        // the previous code, which means thumbnails AND any encoder
        // reading from those preprocessed buffers will produce
        // slightly different embeddings. Easier to wipe and re-encode
        // than to live with a hybrid library where some embeddings
        // came from the old buffers and some from the new.
        //
        // R8 also lands here: the legacy images.embedding column is
        // no longer written by the encoder pipeline (CLIP only writes
        // to the per-encoder embeddings table). The bump triggers a
        // wipe of the legacy column so the cosine populate fallback
        // (cosine/index.rs) sees an empty legacy column and reads only
        // the per-encoder rows.
        const CURRENT_PIPELINE_VERSION: i64 = 3;

        let conn = self.connection.lock().unwrap();

        // Ensure meta table exists.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        let stored: Option<i64> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'embedding_pipeline_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| s.parse::<i64>().ok());

        let needs_migration =
            !matches!(stored, Some(v) if v >= CURRENT_PIPELINE_VERSION);

        if !needs_migration {
            return Ok(());
        }

        info!(
            "Embedding pipeline version migration: stored={:?}, current={} — \
             wiping legacy embeddings to trigger re-encode",
            stored, CURRENT_PIPELINE_VERSION
        );

        // Wipe legacy CLIP from images.embedding column.
        let cleared_legacy = conn.execute(
            "UPDATE images SET embedding = NULL WHERE embedding IS NOT NULL",
            [],
        )?;
        info!("  cleared {} legacy CLIP embeddings from images.embedding", cleared_legacy);

        // Wipe per-encoder rows for the encoders whose pipelines changed.
        // SigLIP-2 rows weren't produced before this version (encoder
        // wasn't wired), so no cleanup needed there.
        let cleared_clip = conn.execute(
            "DELETE FROM embeddings WHERE encoder_id = 'clip_vit_b_32'",
            [],
        )?;
        let cleared_small = conn.execute(
            "DELETE FROM embeddings WHERE encoder_id = 'dinov2_small'",
            [],
        )?;
        info!(
            "  cleared {} clip_vit_b_32 + {} dinov2_small embeddings from embeddings table",
            cleared_clip, cleared_small
        );

        // Mark migration complete.
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('embedding_pipeline_version', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![CURRENT_PIPELINE_VERSION.to_string()],
        )?;

        info!(
            "Embedding pipeline migration complete (version → {}). \
             Next indexing pass will re-encode all images.",
            CURRENT_PIPELINE_VERSION
        );
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
