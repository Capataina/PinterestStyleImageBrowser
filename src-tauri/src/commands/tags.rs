use tauri::State;

use crate::commands::ApiError;
use crate::db::ImageDatabase;
use crate::tag_struct::Tag;

#[tauri::command]
#[tracing::instrument(name = "ipc.get_tags", skip(db))]
pub fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, ApiError> {
    Ok(db.get_tags()?)
}

#[tauri::command]
#[tracing::instrument(name = "ipc.create_tag", skip(db))]
pub fn create_tag(
    db: State<'_, ImageDatabase>,
    name: String,
    color: String,
) -> Result<Tag, ApiError> {
    Ok(db.create_tag(name, color)?)
}

#[tauri::command]
pub fn delete_tag(db: State<'_, ImageDatabase>, tag_id: i64) -> Result<(), ApiError> {
    Ok(db.delete_tag(tag_id)?)
}

#[tauri::command]
pub fn add_tag_to_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), ApiError> {
    Ok(db.add_tag_to_image(image_id, tag_id)?)
}

#[tauri::command]
pub fn remove_tag_from_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), ApiError> {
    Ok(db.remove_tag_from_image(image_id, tag_id)?)
}
