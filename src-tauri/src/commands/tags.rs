use tauri::State;

use crate::db::ImageDatabase;
use crate::tag_struct::Tag;

#[tauri::command]
#[tracing::instrument(name = "ipc.get_tags", skip(db))]
pub fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, String> {
    return db.get_tags().map_err(|e| e.to_string());
}

#[tauri::command]
#[tracing::instrument(name = "ipc.create_tag", skip(db))]
pub fn create_tag(
    db: State<'_, ImageDatabase>,
    name: String,
    color: String,
) -> Result<Tag, String> {
    return db.create_tag(name, color).map_err(|e| e.to_string());
}

#[tauri::command]
pub fn delete_tag(db: State<'_, ImageDatabase>, tag_id: i64) -> Result<(), String> {
    db.delete_tag(tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_tag_to_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), String> {
    db.add_tag_to_image(image_id, tag_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_tag_from_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), String> {
    db.remove_tag_from_image(image_id, tag_id)
        .map_err(|e| e.to_string())
}
