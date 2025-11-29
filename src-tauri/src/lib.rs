use tauri::State;

use crate::{
    db::{ImageDatabase, ID},
    image_struct::ImageData,
    tag_struct::Tag,
};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod db;
pub mod filesystem;
pub mod image_struct;
pub mod similarity_and_semantic_search;
pub mod tag_struct;

#[tauri::command]
fn get_images(
    db: State<'_, ImageDatabase>,
    filter_tag_ids: Vec<ID>,
    filter_string: String,
) -> Result<Vec<ImageData>, String> {
    return db
        .get_images(filter_tag_ids, filter_string)
        .map_err(|e| e.to_string());
}

#[tauri::command]
fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, String> {
    return db.get_tags().map_err(|e| e.to_string());
}

#[tauri::command]
fn create_tag(db: State<'_, ImageDatabase>, name: String, color: String) -> Result<Tag, String> {
    return db.create_tag(name, color).map_err(|e| e.to_string());
}

#[tauri::command]
fn add_tag_to_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), String> {
    db.add_tag_to_image(image_id, tag_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_tag_from_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), String> {
    db.remove_tag_from_image(image_id, tag_id)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(db: ImageDatabase) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(db)
        .invoke_handler(tauri::generate_handler![
            get_images,
            get_tags,
            create_tag,
            add_tag_to_image,
            remove_tag_from_image
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
