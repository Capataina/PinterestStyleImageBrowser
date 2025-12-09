use std::sync::Mutex;
use tauri::State;

use crate::{
    db::{ImageDatabase, ID},
    image_struct::ImageData,
    similarity_and_semantic_search::cosine_similarity::CosineIndex,
    tag_struct::Tag,
};

#[derive(serde::Serialize)]
struct SimilarImage {
    id: ID,
    path: String,
    score: f32,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod db;
pub mod filesystem;
pub mod image_struct;
pub mod similarity_and_semantic_search;
pub mod tag_struct;

pub struct CosineIndexState {
    pub index: Mutex<CosineIndex>,
    pub db_path: String,
}

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

#[tauri::command]
fn get_similar_images(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    image_id: i64,
    top_n: usize,
) -> Result<Vec<SimilarImage>, String> {
    use ndarray::Array1;

    let mut index = cosine_state
        .index
        .lock()
        .map_err(|e| format!("Failed to lock cosine index: {e}"))?;

    if index.cached_images.is_empty() {
        index.populate_from_db(&cosine_state.db_path);
    }

    let embedding = db
        .get_image_embedding(image_id)
        .map_err(|e| format!("Failed to fetch embedding: {e}"))?;

    let query = Array1::from_vec(embedding);
    let results = index
        .get_similar_images(&query, top_n)
        .into_iter()
        .filter_map(|(path, score)| {
            let path_str = path.to_string_lossy().to_string();
            match db.get_image_id_by_path(&path_str) {
                Ok(id) => Some(SimilarImage {
                    id,
                    path: path_str,
                    score,
                }),
                Err(_) => None,
            }
        })
        .collect();

    Ok(results)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(db: ImageDatabase, db_path: String) {
    let cosine_state = CosineIndexState {
        index: Mutex::new(CosineIndex::new()),
        db_path: db_path.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(db)
        .manage(cosine_state)
        .invoke_handler(tauri::generate_handler![
            get_images,
            get_tags,
            create_tag,
            add_tag_to_image,
            remove_tag_from_image,
            get_similar_images
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
