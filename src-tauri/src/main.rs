// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::{db::ImageDatabase, *};
use similarity_and_semantic_search::encoder;
use std::path::Path;
use thumbnail::ThumbnailGenerator;

fn index_directory(path: &std::path::Path, db: &mut ImageDatabase) {
    let scanner = filesystem::ImageScanner::new();
    match scanner.scan_directory(path) {
        Ok(paths) => {
            for path in paths {
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

    // Generate thumbnails for all missing images (before encoding for faster startup)
    println!("=== Generating Thumbnails ===");
    let thumbnail_dir = Path::new(".thumbnails");
    let thumbnail_generator = ThumbnailGenerator::new(
        thumbnail_dir,
        400, // max width for thumbnails
        400, // max height for thumbnails
    )
    .expect("failed to create thumbnail generator");

    thumbnail_generator
        .generate_all_missing_thumbnails(&database)
        .expect("failed to generate thumbnails");

    // Encode images for similarity search
    let mut encoder = encoder::Encoder::new(Path::new("models/model_image.onnx")).unwrap();
    encoder
        .encode_all_images_in_database(32, &database)
        .unwrap();

    image_browser_lib::run(database, db_path)
}
