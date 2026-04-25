// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::db;

fn main() {
    // Pre-Tauri startup work is now minimal: open the SQLite handle and
    // ensure the schema is current. Everything that takes time (model
    // download, scan, thumbnails, embeddings) moved into the indexing
    // pipeline that the Tauri setup() callback spawns on a background
    // thread — see lib.rs::run and indexing.rs.
    //
    // The window opens immediately and the user sees progress over the
    // `indexing-progress` event (Pass 5b renders this as a status pill).
    let db_path = db::ImageDatabase::default_database_path();
    let database = db::ImageDatabase::new(&db_path).expect("failed to init db");
    database.initialize().expect("failed to create tables");

    image_browser_lib::run(database, db_path);
}
