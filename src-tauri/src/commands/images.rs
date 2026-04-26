use tauri::State;

use crate::commands::ApiError;
use crate::db::{ImageDatabase, ID};
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
