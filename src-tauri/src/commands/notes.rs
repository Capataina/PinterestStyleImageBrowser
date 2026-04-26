use tauri::State;

use crate::db::ImageDatabase;

/// Read the free-text annotation for an image. Returns "" if there
/// is no annotation set (the column is either NULL or "" — we treat
/// both as "no annotation" at the user-facing level).
#[tauri::command]
pub fn get_image_notes(
    db: State<'_, ImageDatabase>,
    image_id: i64,
) -> Result<String, String> {
    db.get_image_notes(image_id)
        .map(|opt| opt.unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Write an annotation for an image. Empty / whitespace-only string
/// clears the field.
#[tauri::command]
pub fn set_image_notes(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    notes: String,
) -> Result<(), String> {
    db.set_image_notes(image_id, &notes)
        .map_err(|e| e.to_string())
}
