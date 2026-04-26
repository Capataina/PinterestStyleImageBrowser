//! Background indexing pipeline.
//!
//! Owns the scan → thumbnail → encode flow that previously ran inside
//! main(). Now spawned from a background thread so the Tauri window
//! opens immediately and the user sees progress over the IPC event
//! channel rather than staring at a blank terminal.
//!
//! Two trigger paths:
//!
//! 1. App startup (`run` in lib.rs) — if settings.json has a scan_root
//!    and the model files exist on disk, the setup callback spawns
//!    `run_pipeline` immediately so the catalog refreshes whenever the
//!    user reopens the app.
//! 2. `set_scan_root` IPC command — the user picks a new folder; the
//!    DB is wiped and the same pipeline is spawned to populate it.
//!
//! Concurrency model: a single AtomicBool guards "one indexing run at
//! a time". Trying to start a second run while one is in flight returns
//! `Err(IndexingError::AlreadyRunning)` to the caller. This is a
//! deliberately simple single-flight policy — Pass 5b's UI surfaces
//! the rejection cleanly. A future pass can add cooperative
//! cancellation (cancel-and-restart on rapid root switches) once we
//! see the need.
//!
//! Events: every state change emits a `indexing-progress` Tauri event
//! with an `IndexingProgress` payload. The frontend hook in Pass 5b
//! listens and renders a status pill.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info, warn};

use crate::db::ImageDatabase;
use crate::filesystem::ImageScanner;
use crate::model_download;
use crate::paths;
use crate::similarity_and_semantic_search::cosine_similarity::CosineIndex;
use crate::similarity_and_semantic_search::encoder::Encoder;
use crate::similarity_and_semantic_search::encoder_text::TextEncoder;
use crate::thumbnail::ThumbnailGenerator;
use crate::TextEncoderState;

/// The single-flight guard. Wrap in Arc and stash in a Tauri state
/// struct so commands and the setup callback can both reach it.
#[derive(Default)]
pub struct IndexingState {
    pub is_running: AtomicBool,
}

impl IndexingState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Tauri event payload broadcast as the pipeline progresses.
#[derive(Serialize, Clone, Debug)]
pub struct IndexingProgress {
    pub phase: Phase,
    /// How many units of the current phase have been processed.
    pub processed: usize,
    /// Total units in the current phase. Zero means "indeterminate".
    pub total: usize,
    /// Optional human-readable message — paths, error strings, etc.
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Recursively walking the scan root.
    Scan,
    /// Downloading missing model files.
    ModelDownload,
    /// Generating thumbnails for new images.
    Thumbnail,
    /// Producing CLIP image embeddings batch by batch.
    Encode,
    /// Pipeline complete; cosine index repopulated; UI may refresh.
    Ready,
    /// A non-recoverable error stopped the pipeline. `message` carries
    /// a human-readable string. `is_running` has already been cleared
    /// — the user can retry by switching folders or restarting.
    Error,
}

#[derive(Debug)]
pub enum IndexingError {
    AlreadyRunning,
}

impl std::fmt::Display for IndexingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexingError::AlreadyRunning => {
                write!(f, "Indexing is already in progress; wait for it to finish")
            }
        }
    }
}

impl std::error::Error for IndexingError {}

/// Try to start a background indexing run.
///
/// Returns `Ok(())` immediately if a thread was spawned. Returns
/// `Err(AlreadyRunning)` without doing anything if a run is already in
/// flight.
///
/// The spawned thread:
/// 1. Sets `is_running = true` (already done before spawn — see below).
/// 2. Runs scan, model download, thumbnail, encode in order, emitting
///    events between phases and periodically inside long phases.
/// 3. Repopulates the cosine index from the DB so similarity search
///    works without waiting for the next user-triggered query.
/// 4. Emits `Phase::Ready` with the final image count.
/// 5. Sets `is_running = false`.
///
/// On any error inside the pipeline, the thread emits `Phase::Error`
/// with a message and clears `is_running`.
pub fn try_spawn_pipeline(
    app: AppHandle,
    state: Arc<IndexingState>,
    db_path: String,
    cosine_index: Arc<std::sync::Mutex<CosineIndex>>,
) -> Result<(), IndexingError> {
    // Acquire the single-flight slot atomically.
    if state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(IndexingError::AlreadyRunning);
    }

    thread::spawn(move || {
        // RAII guard ensures is_running gets cleared even if the body
        // panics, so a panic doesn't leave the app permanently locked.
        struct RunningGuard(Arc<IndexingState>);
        impl Drop for RunningGuard {
            fn drop(&mut self) {
                self.0.is_running.store(false, Ordering::SeqCst);
            }
        }
        let _guard = RunningGuard(state.clone());

        if let Err(e) = run_pipeline_inner(&app, &db_path, &cosine_index) {
            error!("pipeline error: {e}");
            emit(
                &app,
                Phase::Error,
                0,
                0,
                Some(format!("Indexing failed: {e}")),
            );
        }
    });

    Ok(())
}

/// The actual pipeline body. Errors propagate up and become a
/// `Phase::Error` event in the spawning closure.
#[tracing::instrument(name = "pipeline.run", skip(app, cosine_index))]
fn run_pipeline_inner(
    app: &AppHandle,
    db_path: &str,
    cosine_index: &Arc<std::sync::Mutex<CosineIndex>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 0. Try the on-disk cosine cache before doing anything else. On a
    //    typical second-launch with no new images, the cache is fresh
    //    and similarity / semantic search become available essentially
    //    immediately — the user can act on the catalog while the rest
    //    of the pipeline (rescan, model verification) finishes in the
    //    background. The cache is invalidated automatically if the DB
    //    file is newer than it.
    {
        let db_path_buf = std::path::PathBuf::from(db_path);
        if let Ok(mut idx) = cosine_index.lock() {
            if idx.cached_images.is_empty() {
                idx.load_from_disk_if_fresh(&db_path_buf);
            }
        }
    }

    // 1. Make sure the model files are on disk. The download function
    //    now reports progress via callback so the UI's status pill can
    //    render a real determinate bar across the ~1 GB of downloads
    //    instead of the previous "Checking models..." flash followed
    //    by a multi-minute silent stretch.
    emit(app, Phase::ModelDownload, 0, 0, Some("Checking models...".into()));
    let app_for_progress = app.clone();
    let progress_cb = move |processed: u64, total: u64, current_file: Option<&str>| {
        let msg = current_file.map(|f| {
            if total > 0 {
                format!(
                    "Downloading {f} — {} / {} MB",
                    processed / 1_048_576,
                    total / 1_048_576
                )
            } else {
                format!("Downloading {f} — {} MB", processed / 1_048_576)
            }
        });
        // Tauri events take usize; cast carefully. The download is
        // capped at ~1.2 GB so usize::MAX is not in play even on 32-bit.
        let processed = processed.min(usize::MAX as u64) as usize;
        let total = total.min(usize::MAX as u64) as usize;
        emit(&app_for_progress, Phase::ModelDownload, processed, total, msg);
    };
    if let Err(e) = model_download::download_models_if_missing(progress_cb) {
        // Non-fatal: scan + thumbnail still work without models. Encode
        // gets skipped further down.
        warn!("model download skipped: {e}");
        emit(
            app,
            Phase::ModelDownload,
            0,
            0,
            Some(format!("Model download skipped: {e}")),
        );
    }

    // 1b. Pre-warm the text encoder so the user's first semantic search
    //     doesn't pause for ~1-2 seconds while the ONNX session and
    //     tokenizer load. We're already on a background thread inside
    //     the pipeline; absorbing the load cost here is invisible to
    //     the user (the indexing pill is already showing), whereas
    //     paying it later means the user types a query and stares at
    //     a spinner.
    {
        let models_dir = paths::models_dir();
        let model_path = models_dir.join("model_text.onnx");
        let tokenizer_path = models_dir.join("tokenizer.json");
        if model_path.exists() && tokenizer_path.exists() {
            // Bind the State separately so its lifetime extends across
            // the full block. Inlining `app.state::<TextEncoderState>()
            // .encoder.lock()` produces a temporary that the borrow
            // checker drops too early.
            let text_encoder_state: tauri::State<'_, TextEncoderState> =
                app.state::<TextEncoderState>();
            let lock_result = text_encoder_state.encoder.lock();
            if let Ok(mut lock) = lock_result {
                if lock.is_none() {
                    info!("pre-warming text encoder");
                    match TextEncoder::new(&model_path, &tokenizer_path) {
                        Ok(encoder) => *lock = Some(encoder),
                        Err(e) => warn!("text encoder pre-warm failed: {e}"),
                    }
                }
            }
        }
    }

    // 2. Open a fresh DB handle. Mutex<Connection> coexists with the
    //    Tauri-managed one (rusqlite supports multiple connections to
    //    the same file).
    let database = ImageDatabase::new(db_path)?;
    database.initialize()?;

    // 3. Resolve the list of roots to scan. Multi-folder support means
    //    we walk every enabled root and tag each image with its source.
    let all_roots = database.list_roots()?;
    let enabled_roots: Vec<_> = all_roots.iter().filter(|r| r.enabled).collect();
    if enabled_roots.is_empty() {
        // Nothing to do — empty-state UI covers this case.
        emit(
            app,
            Phase::Ready,
            0,
            0,
            Some("No folders configured".into()),
        );
        return Ok(());
    }

    // 4. Scan every enabled root. We aggregate paths across roots so
    //    progress reflects total work, not per-folder progress.
    emit(
        app,
        Phase::Scan,
        0,
        0,
        Some(format!("Scanning {} folder(s)", enabled_roots.len())),
    );
    let scanner = ImageScanner::new();

    // First pass: walk every enabled root, collect (path, root_id)
    // tuples. We keep per-root path sets so we can run the orphan
    // detection pass per-root in a moment.
    let mut all_paths: Vec<(String, i64)> = Vec::new();
    let mut paths_per_root: std::collections::HashMap<i64, Vec<String>> =
        std::collections::HashMap::new();
    for root in &enabled_roots {
        let root_path = std::path::Path::new(&root.path);
        if !root_path.exists() {
            warn!(
                "configured root {} no longer exists; skipping",
                root.path
            );
            continue;
        }
        match scanner.scan_directory(root_path) {
            Ok(paths) => {
                let entry = paths_per_root.entry(root.id).or_default();
                for p in paths {
                    entry.push(p.clone());
                    all_paths.push((p, root.id));
                }
            }
            Err(e) => {
                warn!("scan of {} failed: {e}", root.path);
            }
        }
    }
    let total_found = all_paths.len();

    // Second pass: insert into DB. Idempotent — INSERT OR IGNORE on the
    // path uniqueness constraint means existing rows aren't duplicated.
    for (i, (path, root_id)) in all_paths.iter().enumerate() {
        database.add_image(path.clone(), Some(*root_id))?;
        if (i + 1) % 100 == 0 || i + 1 == total_found {
            emit(app, Phase::Scan, i + 1, total_found, None);
        }
    }

    // Orphan-detection pass: for each enabled root, mark any DB row
    // whose path isn't in the just-scanned alive set as orphaned. The
    // grid query filters orphaned rows out, so the user doesn't see
    // tiles for files that were deleted between launches.
    for root in &enabled_roots {
        let alive = paths_per_root.remove(&root.id).unwrap_or_default();
        match database.mark_orphaned(root.id, &alive) {
            Ok(n) if n > 0 => {
                info!("orphan-detection: {} rows marked orphaned in root {}", n, root.path);
            }
            Ok(_) => {}
            Err(e) => warn!("orphan-detection for root {} failed: {e}", root.path),
        }
    }

    emit(app, Phase::Scan, total_found, total_found, None);

    // 5. Thumbnails (parallel via rayon).
    //
    //    Per-image cost is dominated by JPEG decode+encode, which is
    //    embarrassingly parallel. The DB write under the mutex is
    //    microseconds vs ~100ms decode/encode, so contention there is
    //    negligible. On an M-series chip with 8-12 cores this gives
    //    a ~6-10x speedup vs the previous serial loop.
    let thumbnail_generator = ThumbnailGenerator::new(&paths::thumbnails_dir(), 400, 400)?;
    let needs_thumbs = database.get_images_without_thumbnails()?;
    let total_thumbs = needs_thumbs.len();
    if total_thumbs > 0 {
        emit(app, Phase::Thumbnail, 0, total_thumbs, None);

        let completed = AtomicUsize::new(0);
        let last_emit_bucket = AtomicUsize::new(0);
        // Coalesce progress emits to roughly every 25 thumbnails
        // (across ALL workers combined) so a 1500-image run fires
        // ~60 events rather than ~1500.
        const EMIT_EVERY: usize = 25;

        // Build a map from path -> root_id so each thumbnail lands in
        // the right per-root subfolder. Single SELECT — was N+1 before
        // (audit finding): `get_root_id_by_path` per image-needing-
        // thumbnail held the DB Mutex 1500 times in rapid succession on
        // a typical first run. The new `get_paths_to_root_ids` returns
        // the entire (path, root_id) map in one query, matching the
        // pattern `populate_from_db` already uses for embeddings.
        //
        // unwrap_or_default preserves the previous failure semantic:
        // if the SELECT fails, downstream `generate_thumbnail` falls
        // back to the legacy flat thumbnail directory (root_id None).
        let path_to_root = database.get_paths_to_root_ids().unwrap_or_default();

        needs_thumbs.par_iter().for_each(|image| {
            let root_id = path_to_root.get(&image.path).copied().flatten();
            match thumbnail_generator.generate_thumbnail(
                Path::new(&image.path),
                image.id,
                root_id,
            ) {
                Ok(result) => {
                    if let Err(e) = database.update_image_thumbnail(
                        image.id,
                        &result.thumbnail_path,
                        result.original_width,
                        result.original_height,
                    ) {
                        warn!("DB update for thumbnail of image {} failed: {e}", image.id);
                    }
                }
                Err(e) => {
                    warn!("thumbnail generation failed for {}: {e}", image.path);
                }
            }

            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let bucket = done / EMIT_EVERY;
            let prev = last_emit_bucket.load(Ordering::Relaxed);
            if bucket > prev
                && last_emit_bucket
                    .compare_exchange(prev, bucket, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                emit(app, Phase::Thumbnail, done, total_thumbs, None);
            }
        });
    }
    emit(app, Phase::Thumbnail, total_thumbs, total_thumbs, None);

    // 6. Encode embeddings (only if the model is available).
    let image_model_path = paths::models_dir().join("model_image.onnx");
    if image_model_path.exists() {
        let needs_embed = database.get_images_without_embeddings()?;
        let total_embed = needs_embed.len();
        if total_embed > 0 {
            emit(app, Phase::Encode, 0, total_embed, None);
            let mut encoder = Encoder::new(&image_model_path)?;
            // Match the existing batch size from main.rs.
            const BATCH_SIZE: usize = 32;
            let mut processed = 0usize;
            for chunk in needs_embed.chunks(BATCH_SIZE) {
                let batch_paths: Vec<&Path> =
                    chunk.iter().map(|i| Path::new(&i.path)).collect();
                let embeddings = encoder.encode_batch(&batch_paths)?;
                for (image, embedding) in chunk.iter().zip(embeddings.iter()) {
                    database.update_image_embedding(image.id, embedding.clone())?;
                }
                processed += chunk.len();
                emit(app, Phase::Encode, processed, total_embed, None);
            }
        }
    } else {
        warn!(
            "model_image.onnx missing; embeddings will be \
             empty until next launch."
        );
    }

    // 7. Repopulate the in-memory cosine cache from the now-fresh
    //    embeddings, then persist to disk so next-launch starts hot.
    if let Ok(mut idx) = cosine_index.lock() {
        idx.populate_from_db(&database);
        idx.save_to_disk();
    }

    // 8. Done — total image count is what the user sees in the grid.
    let final_count = database.get_all_images().map(|v| v.len()).unwrap_or(0);
    emit(
        app,
        Phase::Ready,
        final_count,
        final_count,
        Some(format!("{final_count} images indexed")),
    );

    Ok(())
}

fn emit(
    app: &AppHandle,
    phase: Phase,
    processed: usize,
    total: usize,
    message: Option<String>,
) {
    let payload = IndexingProgress {
        phase,
        processed,
        total,
        message,
    };
    if let Err(e) = app.emit("indexing-progress", &payload) {
        // Don't crash on emit failure — just log. Receivers may have
        // disconnected (closing window mid-pipeline).
        warn!("failed to emit event: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_state_default_not_running() {
        let s = IndexingState::new();
        assert!(!s.is_running.load(Ordering::SeqCst));
    }

    #[test]
    fn phase_serialises_kebab_case() {
        let progress = IndexingProgress {
            phase: Phase::ModelDownload,
            processed: 1,
            total: 3,
            message: None,
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(
            json.contains("\"phase\":\"model-download\""),
            "expected kebab-case phase, got {json}"
        );
    }

    #[test]
    fn ready_phase_serialises() {
        let progress = IndexingProgress {
            phase: Phase::Ready,
            processed: 42,
            total: 42,
            message: Some("done".into()),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"phase\":\"ready\""));
        assert!(json.contains("\"processed\":42"));
        assert!(json.contains("\"message\":\"done\""));
    }

    #[test]
    fn single_flight_first_acquire_succeeds() {
        // Direct test of the AtomicBool gate semantics that
        // try_spawn_pipeline relies on. We don't actually spawn the
        // pipeline (it'd need a Tauri app handle) — just exercise
        // the compare_exchange behaviour.
        let state = IndexingState::new();
        let acquired = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(acquired.is_ok());
        // Now the slot is held; second attempt should fail.
        let denied = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(denied.is_err());
    }

    #[test]
    fn single_flight_releases_after_clear() {
        let state = IndexingState::new();
        state.is_running.store(true, Ordering::SeqCst);
        // Simulate the RAII guard's drop behaviour.
        state.is_running.store(false, Ordering::SeqCst);
        // The slot is open again.
        let reacquired = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(reacquired.is_ok());
    }

    #[test]
    fn indexing_error_displays_human_readable_message() {
        let err = IndexingError::AlreadyRunning;
        let msg = format!("{err}");
        assert!(
            msg.contains("already in progress"),
            "expected human-readable AlreadyRunning message, got {msg}"
        );
    }

    #[test]
    fn all_phases_serialise_to_kebab_case() {
        for (variant, expected_str) in [
            (Phase::Scan, "scan"),
            (Phase::ModelDownload, "model-download"),
            (Phase::Thumbnail, "thumbnail"),
            (Phase::Encode, "encode"),
            (Phase::Ready, "ready"),
            (Phase::Error, "error"),
        ] {
            let progress = IndexingProgress {
                phase: variant,
                processed: 0,
                total: 0,
                message: None,
            };
            let json = serde_json::to_string(&progress).unwrap();
            let needle = format!("\"phase\":\"{}\"", expected_str);
            assert!(
                json.contains(&needle),
                "Phase {expected_str:?} did not serialise as expected: {json}"
            );
        }
    }
}
