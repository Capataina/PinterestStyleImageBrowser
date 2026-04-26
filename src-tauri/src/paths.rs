//! Platform-default paths for app data, thumbnails, models, settings,
//! and exports.
//!
//! All user-state lives under the platform's standard app-data
//! directory regardless of build mode (debug or release). Layout:
//!
//! ```text
//! <platform_data_dir>/com.ataca.image-browser/
//!   images.db
//!   settings.json
//!   cosine_cache.bin
//!   models/
//!     model_image.onnx
//!     model_text.onnx
//!     tokenizer.json
//!   thumbnails/
//!     root_1/thumb_42.jpg
//!     root_2/thumb_99.jpg
//!   exports/
//!     perf-<unix-ts>/{report.md,raw.json,timeline.jsonl}
//! ```
//!
//! On macOS the base is `~/Library/Application Support/`; on Linux
//! `$XDG_DATA_HOME` (default `~/.local/share`); on Windows
//! `%APPDATA%`. The bundle id segment matches `tauri.conf.json`.
//!
//! `IMAGE_BROWSER_DATA_DIR` env var overrides the default for ad-hoc
//! redirection (testing, side-by-side instances, CI fixtures).

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Strip the Windows extended-path prefix `\\?\` if present. Returns
/// the input borrow unchanged when the prefix is absent (the common
/// case on every non-Windows platform), so call sites pay no
/// allocation when the path doesn't need normalising.
///
/// Why this exists as a function (not a closure copied per command):
/// audit finding — the same 7-line closure was triplicated across
/// `semantic_search`, `get_similar_images`, and `get_tiered_similar_images`.
/// The project notes already flagged "don't add a fourth normalisation
/// closure"; the third one was already the redundancy. This helper is
/// the single place to fix Windows path handling if the schema ever
/// changes (e.g., normalising at insert time).
pub fn strip_windows_extended_prefix(path_str: &str) -> Cow<'_, str> {
    // 6b — clippy::manual_strip. strip_prefix returns the suffix
    // directly when the prefix matches.
    match path_str.strip_prefix("\\\\?\\") {
        Some(stripped) => Cow::Owned(stripped.to_string()),
        None => Cow::Borrowed(path_str),
    }
}

use tracing::warn;

/// Tauri bundle identifier — must stay in sync with `tauri.conf.json::identifier`.
const BUNDLE_ID: &str = "com.ataca.image-browser";

/// Root of all app-managed state.
///
/// Always resolves to the platform's standard app-data directory so
/// debug and release builds share state. On macOS that's
/// `~/Library/Application Support/com.ataca.image-browser/`; on
/// Linux `$XDG_DATA_HOME/com.ataca.image-browser/`; on Windows
/// `%APPDATA%/com.ataca.image-browser/`.
///
/// The `IMAGE_BROWSER_DATA_DIR` env var overrides this for ad-hoc
/// redirection (testing against a separate state, running multiple
/// instances side by side, pointing CI at a fixture directory). Set
/// it to an absolute path; the env var wins over the platform
/// default.
///
/// Why platform-default rather than project-local: dev and release
/// builds compile differently (debug vs --release) but should see
/// the same DB, the same downloaded models (1.1GB), and the same
/// thumbnails. A `cfg(debug_assertions)` split would force a
/// re-download every time you switched build modes — which is
/// exactly the bite that prompted reverting to the standard path.
pub fn app_data_dir() -> PathBuf {
    // Env-var override comes first — useful for testing against a
    // separate state, running multiple instances, or pointing at a
    // fixture directory.
    if let Ok(override_path) = std::env::var("IMAGE_BROWSER_DATA_DIR") {
        if !override_path.is_empty() {
            let dir = PathBuf::from(override_path);
            let _ = ensure_dir(&dir);
            return dir;
        }
    }

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

/// Top-level directory under which all thumbnails live, organised by
/// root id (Phase 9 reorg per user feedback).
pub fn thumbnails_dir() -> PathBuf {
    let p = app_data_dir().join("thumbnails");
    let _ = ensure_dir(&p);
    p
}

/// Per-root thumbnail directory.
///
/// Layout:
///   app_data_dir/thumbnails/
///     root_1/thumb_42.jpg
///     root_2/thumb_99.jpg
///
/// Per-root segregation means removing a root from the multi-folder
/// list can also cascade-delete its thumbnails on disk in one
/// `rm -rf` rather than per-row file deletion.
///
/// Legacy layout (pre-multi-folder) put every thumbnail flat under
/// `thumbnails/`. Old DBs with that layout still work because the
/// thumbnail_path column stores absolute paths; new thumbnails just
/// land under the per-root subfolder going forward.
pub fn thumbnails_dir_for_root(root_id: i64) -> PathBuf {
    let p = thumbnails_dir().join(format!("root_{root_id}"));
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

/// Path to the on-disk cached cosine index (a bincode-encoded
/// Vec<(PathBuf, Vec<f32>)>). Loaded eagerly at app startup if it
/// exists and is fresher than the SQLite DB; populated again from
/// the DB whenever the indexing pipeline finishes encoding.
///
/// Lives in app_data_dir rather than in the DB itself because the
/// embedding BLOB column already holds the canonical data — the cache
/// just speeds up the load path.
pub fn cosine_cache_path() -> PathBuf {
    app_data_dir().join("cosine_cache.bin")
}

/// User-facing exports directory. Anything the user might want to
/// share, archive, or compare goes here:
///   - perf-<unix-ts>/ — profiling sessions written by perf_report
///   - (future) selected-set.csv, query-history.json, etc.
///
/// Lives under app_data_dir() so it follows the same "all generated
/// files in one place" rule as everything else.
pub fn exports_dir() -> PathBuf {
    let p = app_data_dir().join("exports");
    let _ = ensure_dir(&p);
    p
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
    fn test_app_data_dir_lives_under_platform_data_dir() {
        // app_data_dir() always resolves to <platform_data_dir>/<bundle_id>/
        // regardless of build mode. Verify the basename matches the
        // bundle id (last path segment).
        let dir = app_data_dir();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(BUNDLE_ID),
            "app_data_dir basename should be the bundle id, got: {}",
            dir.display()
        );
    }

    // IMAGE_BROWSER_DATA_DIR override has no automated test because
    // process-wide env mutation races with the other paths tests when
    // cargo runs them in parallel. The override is a one-line read +
    // PathBuf construct; manual smoke-test from a shell:
    //   IMAGE_BROWSER_DATA_DIR=/tmp/foo cargo run --bin image-browser
    // and verify the binary writes images.db under /tmp/foo/.

    #[test]
    fn test_paths_are_under_app_data_dir() {
        let root = app_data_dir();
        assert!(database_path().starts_with(&root));
        assert!(thumbnails_dir().starts_with(&root));
        assert!(models_dir().starts_with(&root));
        assert!(settings_path().starts_with(&root));
    }

    #[test]
    fn thumbnails_dir_for_root_creates_subfolder() {
        // Phase 9 reorganisation — each root gets its own thumbnail
        // subfolder so remove_root can rm -rf it cleanly.
        let dir = thumbnails_dir_for_root(42);
        assert!(dir.exists(), "thumbnails_dir_for_root should create the dir");
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some("root_42")
        );
    }

    #[test]
    fn cosine_cache_path_is_under_app_data_dir() {
        let cache = cosine_cache_path();
        assert!(cache.starts_with(app_data_dir()));
        assert_eq!(
            cache.file_name().and_then(|s| s.to_str()),
            Some("cosine_cache.bin")
        );
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
