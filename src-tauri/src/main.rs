// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::{db::ImageDatabase, *};
use similarity_and_semantic_search::encoder;
use std::path::Path;

fn index_directory(path: &std::path::Path, db: &mut ImageDatabase) {
    let scanner = filesystem::ImageScanner::new();
    match scanner.scan_directory(path) {
        Ok(paths) => {
            for path in paths {
                println!("Found image: {:?}", path);
                db.add_image(path).unwrap();
            }
        }
        Err(e) => {
            eprintln!("Error scanning directory: {}", e);
        }
    }
}

fn main() {
    let test_path = std::path::Path::new("test_images");

    let db_path = db::ImageDatabase::default_database_path();
    let mut database = db::ImageDatabase::new(&db_path).expect("failed to init db");
    database.initialize().expect("failed to create tables");

    index_directory(test_path, &mut database);

    let mut encoder = encoder::Encoder::new(Path::new("models/model.onnx")).unwrap();
    encoder
        .encode_all_images_in_database(32, &mut database)
        .unwrap();
    image_browser_lib::run(database)
}
