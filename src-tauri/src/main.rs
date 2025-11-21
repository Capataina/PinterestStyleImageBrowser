// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::*;

fn index_directory(path: &std::path::Path) {
    let scanner = filesystem::ImageScanner::new();
    match scanner.scan_directory(path) {
        Ok(images) => {
            for image in images {
                println!("Found image: {:?}", image);
            }
        }
        Err(e) => {
            eprintln!("Error scanning directory: {}", e);
        }
    }

    let image_database_path = crate::db::ImageDatabase::default_database_path();
    let mut db = crate::db::ImageDatabase::new(&image_database_path).unwrap();
    db.initialize().unwrap();

    for image in scanner.scan_directory(path).unwrap() {
        db.add_image(image).unwrap();
    }
}

fn main() {
    let test_path = std::path::Path::new("C:\\image-browser\\src-tauri\\test_images");
    index_directory(test_path);
    image_browser_lib::run()
}
