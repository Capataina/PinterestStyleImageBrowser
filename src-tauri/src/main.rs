// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::db;
use tracing_subscriber::{fmt, EnvFilter};

fn main() {
    // Initialise the tracing subscriber.
    //
    // Default filter: this crate at info, the rest of the dep tree at
    // warn (so we don't drown in tao/wry/winit chatter). Override via
    // `RUST_LOG=image_browser_lib=debug` for verbose dev output, or
    // `RUST_LOG=trace` for everything.
    //
    // We deliberately don't `.expect()` the init — if a subscriber is
    // already installed (e.g. during cargo test), we'd rather log
    // through the existing one than crash the binary.
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("warn,image_browser_lib=info,image_browser=info")
            }),
        )
        .with_target(true)
        .try_init();

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
