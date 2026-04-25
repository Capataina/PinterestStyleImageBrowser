//! Platform-correct paths for app data, thumbnails, models, and settings.
//!
//! Everything user-state related lives under one app data directory:
//!
//! - macOS:   `~/Library/Application Support/com.ataca.image-browser/`
//! - Linux:   `~/.local/share/com.ataca.image-browser/`
//! - Windows: `%APPDATA%\com.ataca.image-browser\`
//!
//! The `dirs` crate provides the platform root; we append the Tauri
//! bundle identifier to scope it to this app. Each helper ensures the
//! relevant directory exists before returning the path.

use std::fs;
use std::io;
use std::path::PathBuf;

use tracing::warn;

/// Tauri bundle identifier — must stay in sync with `tauri.conf.json::identifier`.
const BUNDLE_ID: &str = "com.ataca.image-browser";

/// Root of all app-managed state.
///
/// Falls back to the current directory if `dirs::data_dir()` returns None
/// (extremely rare; only on platforms where no data directory is defined).
/// We log to stderr in that case so the user has at least some signal.
pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| {
        warn!(
            "dirs::data_dir() returned None; falling back to ./app-data — \
             your platform may not be fully supported"
        );
        PathBuf::from("./app-data")
    });
    let dir = base.join(BUNDLE_ID);
    let _ = ensure_dir(&dir);
    dir
}

/// Path to the SQLite database file. Parent directory is created if missing.
pub fn database_path() -> PathBuf {
    app_data_dir().join("images.db")
}

/// Directory where thumbnails are cached. Created if missing.
pub fn thumbnails_dir() -> PathBuf {
    let p = app_data_dir().join("thumbnails");
    let _ = ensure_dir(&p);
    p
}

/// Directory where ONNX models and the tokenizer.json live.
/// Created if missing. Pass 4 will download the model files into here
/// on first launch.
pub fn models_dir() -> PathBuf {
    let p = app_data_dir().join("models");
    let _ = ensure_dir(&p);
    p
}

/// Path to the user-facing settings JSON file. The file may not exist
/// yet on first launch; callers should treat absence as "use defaults."
pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

fn ensure_dir(path: &PathBuf) -> io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_data_dir_contains_bundle_id() {
        let dir = app_data_dir();
        assert!(
            dir.to_string_lossy().contains(BUNDLE_ID),
            "app_data_dir should contain the bundle identifier, got: {}",
            dir.display()
        );
    }

    #[test]
    fn test_paths_are_under_app_data_dir() {
        let root = app_data_dir();
        assert!(database_path().starts_with(&root));
        assert!(thumbnails_dir().starts_with(&root));
        assert!(models_dir().starts_with(&root));
        assert!(settings_path().starts_with(&root));
    }

    #[test]
    fn test_filenames_are_stable() {
        assert_eq!(
            database_path().file_name().unwrap().to_string_lossy(),
            "images.db"
        );
        assert_eq!(
            settings_path().file_name().unwrap().to_string_lossy(),
            "settings.json"
        );
    }
}
