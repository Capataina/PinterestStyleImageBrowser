use tauri::State;

use crate::{db::ImageDatabase, image_struct::ImageData, tag_struct::Tag};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod db;
pub mod filesystem;
pub mod image_struct;
pub mod similarity_and_semantic_search;
pub mod tag_struct;

#[tauri::command]
fn get_all_images(db: State<'_, ImageDatabase>) -> Result<Vec<ImageData>, String> {
    return db.get_all_images().map_err(|e| e.to_string());
}

#[tauri::command]
fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, String> {
    return db.get_tags().map_err(|e| e.to_string());
}

#[tauri::command]
fn create_tag(db: State<'_, ImageDatabase>, name: String, color: String) -> Result<Tag, String> {
    return db.create_tag(name, color).map_err(|e| e.to_string());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(db: ImageDatabase) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(db)
        .invoke_handler(tauri::generate_handler![
            get_all_images,
            get_tags,
            create_tag
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
