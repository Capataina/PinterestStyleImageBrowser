use tauri::State;

use crate::{db::ImageDatabase, image_struct::ImageData};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod db;
pub mod filesystem;
pub mod image_struct;
pub mod tag_struct;
pub mod similarity_and_semantic_search;

#[tauri::command]
fn get_all_images(db: State<'_, ImageDatabase>) -> Vec<ImageData> {
    return db.get_all_images().unwrap();
}

// #[tauri::command]
// fn get_all_tags(db: State<'_, ImageDatabase>) -> Vec<Tag> {
//     return db.get_all_images().unwrap();
// }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(db: ImageDatabase) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(db)
        .invoke_handler(tauri::generate_handler![get_all_images])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
