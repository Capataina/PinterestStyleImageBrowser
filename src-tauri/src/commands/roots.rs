use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tracing::{info, warn};

use crate::commands::ApiError;
use crate::db::ImageDatabase;
use crate::indexing::{self, IndexingState};
use crate::paths;
use crate::root_struct::Root;
use crate::settings;
use crate::{CosineIndexState, FusionIndexState};

/// Read the currently-configured scan root from settings.json, if any.
/// Returns Ok(None) when no root has been picked yet (first-launch state).
#[tauri::command]
pub fn get_scan_root() -> Result<Option<String>, ApiError> {
    Ok(settings::Settings::load()
        .scan_root
        .map(|p| p.to_string_lossy().into_owned()))
}

/// Replace every configured root with a single new one and trigger a
/// live re-index. This is what the "Choose folder" button calls — the
/// "I just want one folder, replace what's there" UX. For multi-folder
/// management see `add_root` / `remove_root` / `set_root_enabled`.
///
/// The tag catalogue is preserved across root replacement.
#[tauri::command]
pub fn set_scan_root(
    app: AppHandle,
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    fusion_state: State<'_, FusionIndexState>,
    indexing_state: State<'_, Arc<IndexingState>>,
    path: String,
) -> Result<(), ApiError> {
    let scan_root = PathBuf::from(&path);
    if !scan_root.is_dir() {
        return Err(ApiError::BadInput(format!("Not a directory: {path}")));
    }

    // Remove existing roots (CASCADE deletes their images), wipe any
    // orphan rows (NULL root_id from older DBs), then add the new one.
    let existing = db.list_roots()?;
    for r in existing {
        db.remove_root(r.id)?;
    }
    db.wipe_images_for_new_root()?;
    db.add_root(path.clone())?;

    cosine_state.invalidate();
    // Phase 5 — fusion caches contain entries from the now-removed
    // root; clear so the next fusion call rebuilds against the new
    // image set.
    fusion_state.invalidate_all();

    indexing::try_spawn_pipeline(
        app.clone(),
        indexing_state.inner().clone(),
        cosine_state.db_path.clone(),
        cosine_state.index.clone(),
        cosine_state.current_encoder_id.clone(),
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    info!("set_scan_root replaced roots + spawned indexing.");
    Ok(())
}

/// Multi-folder management — list configured roots.
#[tauri::command]
#[tracing::instrument(name = "ipc.list_roots", skip(db))]
pub fn list_roots(db: State<'_, ImageDatabase>) -> Result<Vec<Root>, ApiError> {
    Ok(db.list_roots()?)
}

/// Add a root and trigger an incremental re-index. Returns the new
/// Root row so the UI can show it immediately without round-tripping
/// list_roots.
#[tauri::command]
pub fn add_root(
    app: AppHandle,
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    indexing_state: State<'_, Arc<IndexingState>>,
    path: String,
) -> Result<Root, ApiError> {
    let scan_root = PathBuf::from(&path);
    if !scan_root.is_dir() {
        return Err(ApiError::BadInput(format!("Not a directory: {path}")));
    }
    let root = db.add_root(path)?;

    indexing::try_spawn_pipeline(
        app.clone(),
        indexing_state.inner().clone(),
        cosine_state.db_path.clone(),
        cosine_state.index.clone(),
        cosine_state.current_encoder_id.clone(),
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    info!("add_root persisted ({}) and spawned re-index.", root.path);
    Ok(root)
}

/// Remove a root. The CASCADE on images.root_id wipes its images;
/// surviving image rows from other roots are unaffected. The root's
/// dedicated thumbnail directory on disk is also recursively
/// deleted so we don't leave orphaned cached files.
#[tauri::command]
pub fn remove_root(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    fusion_state: State<'_, FusionIndexState>,
    id: i64,
) -> Result<(), ApiError> {
    db.remove_root(id)?;
    // Clean the per-root thumbnail subfolder. Best-effort — if the
    // remove fails (permissions, file locked) we log and move on; the
    // user can manually clean the directory.
    let thumbnail_dir = paths::thumbnails_dir_for_root(id);
    if thumbnail_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&thumbnail_dir) {
            warn!(
                "could not remove thumbnail dir {}: {e}",
                thumbnail_dir.display()
            );
        } else {
            info!("removed thumbnail dir {}", thumbnail_dir.display());
        }
    }
    // Cosine cache contains entries from the removed root; cheapest
    // way to clean is to drop the whole cache and let next-query
    // populate from the remaining DB rows.
    cosine_state.invalidate();
    fusion_state.invalidate_all();
    info!("remove_root removed root id {}", id);
    Ok(())
}

/// Toggle a root's enabled flag. No re-index needed — the grid query
/// filters by enabled status, so the toggle is instant.
#[tauri::command]
pub fn set_root_enabled(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    fusion_state: State<'_, FusionIndexState>,
    id: i64,
    enabled: bool,
) -> Result<(), ApiError> {
    db.set_root_enabled(id, enabled)?;
    // Cosine cache may include images from the toggled root; clear so
    // the next similarity query rebuilds with the right active set.
    cosine_state.invalidate();
    fusion_state.invalidate_all();
    Ok(())
}
