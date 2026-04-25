// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::{db::ImageDatabase, *};
use similarity_and_semantic_search::encoder;
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
    // Resolve all user-state paths via the new paths module. Touching these
    // also lazily creates the app data directory tree the first time the
    // app runs.
    let db_path = db::ImageDatabase::default_database_path();
    let thumbnail_dir = paths::thumbnails_dir();
    let models_dir = paths::models_dir();
    let image_model_path = models_dir.join("model_image.onnx");

    let mut database = db::ImageDatabase::new(&db_path).expect("failed to init db");
    database.initialize().expect("failed to create tables");

    // Read scan root from settings.json. None means "no folder picked yet"
    // — Pass 4 will surface a folder-picker UI; for now we just skip the
    // scan/encode pipeline so the app can still launch. The grid will be
    // empty until a root is configured.
    let user_settings = settings::Settings::load();
    if let Some(scan_root) = user_settings.scan_root.clone() {
        if scan_root.exists() {
            println!("=== Indexing {} ===", scan_root.display());
            index_directory(&scan_root, &mut database);

            println!("=== Generating Thumbnails ===");
            let thumbnail_generator =
                ThumbnailGenerator::new(&thumbnail_dir, 400, 400)
                    .expect("failed to create thumbnail generator");
            thumbnail_generator
                .generate_all_missing_thumbnails(&database)
                .expect("failed to generate thumbnails");

            // Encode only if the image model has been downloaded (Pass 4).
            // Without the model, embeddings stay NULL and similarity /
            // semantic search return empty results — but the rest of the
            // app (grid + tags) still works.
            if image_model_path.exists() {
                let mut encoder = encoder::Encoder::new(&image_model_path).unwrap();
                encoder
                    .encode_all_images_in_database(32, &database)
                    .unwrap();
            } else {
                eprintln!(
                    "[main] Image model not found at {} — skipping encode pass. \
                     Similarity and semantic search will be disabled until \
                     models are installed.",
                    image_model_path.display()
                );
            }
        } else {
            eprintln!(
                "[main] Configured scan_root {} no longer exists — skipping. \
                 Re-pick a folder from the UI to re-index.",
                scan_root.display()
            );
        }
    } else {
        println!(
            "[main] No scan_root in settings yet — launch the app and pick a folder."
        );
    }

    image_browser_lib::run(database, db_path)
}
