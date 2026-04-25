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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{error, warn};

use crate::db::ImageDatabase;
use crate::filesystem::ImageScanner;
use crate::model_download;
use crate::paths;
use crate::similarity_and_semantic_search::cosine_similarity::CosineIndex;
use crate::similarity_and_semantic_search::encoder::Encoder;
use crate::thumbnail::ThumbnailGenerator;

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
fn run_pipeline_inner(
    app: &AppHandle,
    db_path: &str,
    cosine_index: &Arc<std::sync::Mutex<CosineIndex>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Make sure the model files are on disk. No-op on subsequent runs.
    emit(app, Phase::ModelDownload, 0, 0, Some("Checking models...".into()));
    if let Err(e) = model_download::download_models_if_missing() {
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

    // 2. Resolve the scan root from settings. If none, exit cleanly.
    let user_settings = crate::settings::Settings::load();
    let scan_root = match user_settings.scan_root {
        Some(p) if p.exists() => p,
        Some(p) => {
            return Err(format!(
                "Configured scan_root {} no longer exists; pick a new folder",
                p.display()
            )
            .into())
        }
        None => {
            // Nothing to do — the UI's empty state covers the no-folder case.
            emit(app, Phase::Ready, 0, 0, Some("No folder configured".into()));
            return Ok(());
        }
    };

    // 3. Open a fresh DB handle. Mutex<Connection> coexists with the
    //    Tauri-managed one (rusqlite supports multiple connections to the
    //    same file). This mirrors the cosine module's existing pattern.
    let database = ImageDatabase::new(db_path)?;
    database.initialize()?;

    // 4. Scan.
    emit(
        app,
        Phase::Scan,
        0,
        0,
        Some(format!("Scanning {}", scan_root.display())),
    );
    let scanner = ImageScanner::new();
    let paths_found = scanner.scan_directory(&scan_root)?;
    let total_found = paths_found.len();
    for (i, path) in paths_found.iter().enumerate() {
        database.add_image(path.clone())?;
        // Lightweight progress: every 100 images is plenty.
        if (i + 1) % 100 == 0 || i + 1 == total_found {
            emit(app, Phase::Scan, i + 1, total_found, None);
        }
    }
    emit(app, Phase::Scan, total_found, total_found, None);

    // 5. Thumbnails.
    let thumbnail_generator = ThumbnailGenerator::new(&paths::thumbnails_dir(), 400, 400)?;
    let needs_thumbs = database.get_images_without_thumbnails()?;
    let total_thumbs = needs_thumbs.len();
    if total_thumbs > 0 {
        emit(app, Phase::Thumbnail, 0, total_thumbs, None);
        for (i, image) in needs_thumbs.iter().enumerate() {
            // Reuse the existing per-image generation; the bulk method
            // would also work but doesn't expose granular progress.
            match thumbnail_generator.generate_thumbnail(Path::new(&image.path), image.id) {
                Ok(result) => {
                    database.update_image_thumbnail(
                        image.id,
                        &result.thumbnail_path,
                        result.original_width,
                        result.original_height,
                    )?;
                }
                Err(e) => {
                    warn!(
                        "thumbnail failed for {}: {e}",
                        image.path
                    );
                }
            }
            if (i + 1) % 25 == 0 || i + 1 == total_thumbs {
                emit(app, Phase::Thumbnail, i + 1, total_thumbs, None);
            }
        }
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

    // 7. Repopulate the in-memory cosine cache. The cosine module owns
    //    the populate logic; we just clear-and-trigger.
    if let Ok(mut idx) = cosine_index.lock() {
        idx.cached_images.clear();
        idx.populate_from_db(db_path);
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
}
