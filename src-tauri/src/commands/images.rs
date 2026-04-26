use tauri::State;

use crate::commands::ApiError;
use crate::db::{images_query::PipelineStats, ImageDatabase, ID};
use crate::image_struct::ImageData;

#[tauri::command]
#[tracing::instrument(name = "ipc.get_images", skip(db), fields(tag_count = filter_tag_ids.len()))]
pub fn get_images(
    db: State<'_, ImageDatabase>,
    filter_tag_ids: Vec<ID>,
    filter_string: String,
    match_all_tags: Option<bool>,
) -> Result<Vec<ImageData>, ApiError> {
    // match_all_tags is Option so older frontend builds (or tests)
    // can call without specifying — defaults to false (OR semantic).
    let match_all = match_all_tags.unwrap_or(false);
    Ok(db.get_images_with_thumbnails(filter_tag_ids, filter_string, match_all)?)
}

/// Snapshot of pipeline progress — counts of images at each stage
/// (total / with-thumbnail / with-embedding / orphaned). Surfaced in
/// the SettingsDrawer so the user can see how much work the indexing
/// pipeline has done; also useful for verifying the (planned) parallel
/// thumbnail+encoding worker design is making progress on both queues.
///
/// Single SELECT — one DB Mutex acquire regardless of library size.
#[tauri::command]
#[tracing::instrument(name = "ipc.get_pipeline_stats", skip(db))]
pub fn get_pipeline_stats(db: State<'_, ImageDatabase>) -> Result<PipelineStats, ApiError> {
    Ok(db.get_pipeline_stats()?)
}
