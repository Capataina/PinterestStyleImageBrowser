//! Project-local paths for app data, thumbnails, models, and settings.
//!
//! Everything user-state lives under `<repo>/Library/` in dev builds —
//! the user prefers project-local visibility over the standard
//! platform-convention paths. Layout:
//!
//! ```text
//! <repo>/Library/
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
//! ```
//!
//! Library/ is gitignored so user state never lands in commits.
//!
//! Trade-off: a packaged release build (no project folder, no
//! CARGO_MANIFEST_DIR) needs the platform app-data dir as a fallback.
//! We branch on `cfg(debug_assertions)` to do the right thing:
//! - `cargo tauri dev` (debug)   → <repo>/Library/
//! - `tauri build` (release)     → ~/Library/Application Support/<bundle_id>/

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
    if path_str.starts_with("\\\\?\\") {
        Cow::Owned(path_str[4..].to_string())
    } else {
        Cow::Borrowed(path_str)
    }
}

#[cfg(not(debug_assertions))]
use tracing::warn;

/// Tauri bundle identifier — must stay in sync with `tauri.conf.json::identifier`.
/// Used only by the release-build fallback path.
#[cfg(not(debug_assertions))]
const BUNDLE_ID: &str = "com.ataca.image-browser";

/// Root of all app-managed state.
///
/// Resolution order (first match wins):
///
/// 1. `IMAGE_BROWSER_DATA_DIR` env var, if set. Lets you point ANY
///    build (release or debug) at any directory — useful when running
///    a release-mode profiling session against your dev `<repo>/Library/`
///    so you don't re-download the 1.1GB models. Set it to an absolute
///    path:
///        IMAGE_BROWSER_DATA_DIR=/path/to/repo/Library cargo tauri dev --release
/// 2. Debug builds resolve to `<repo>/Library/` via `CARGO_MANIFEST_DIR`
///    (captured at compile time; points at `src-tauri/`, so `.parent()`
///    lands at the repo root).
/// 3. Release builds fall back to the platform's app data directory
///    (`~/Library/Application Support/com.ataca.image-browser/` on
///    macOS) because a packaged binary has no project folder.
pub fn app_data_dir() -> PathBuf {
    // Env-var override comes first so it works in EITHER cfg branch.
    // The whole point of this escape hatch is to let release-mode
    // dev runs (cargo tauri dev --release) point at the dev data
    // dir, sharing the model cache + DB across build modes.
    if let Ok(override_path) = std::env::var("IMAGE_BROWSER_DATA_DIR") {
        if !override_path.is_empty() {
            let dir = PathBuf::from(override_path);
            let _ = ensure_dir(&dir);
            return dir;
        }
    }

    #[cfg(debug_assertions)]
    {
        // CARGO_MANIFEST_DIR is captured by env! at compile time; it
        // points at src-tauri/ (where Cargo.toml lives). One step up
        // is the repo root, where Library/ lives.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = std::path::Path::new(manifest_dir)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = repo_root.join("Library");
        let _ = ensure_dir(&dir);
        return dir;
    }

    #[cfg(not(debug_assertions))]
    {
        // Release: a packaged binary doesn't have a project folder.
        // Fall back to the platform's app-data directory so the
        // built app still works when distributed.
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
///   - perf-<unix-ts>.json — perf snapshots from the diagnostics overlay
///   - (future) selected-set.csv, query-history.json, etc.
///
/// Lives under Library/ so it follows the same "all generated files
/// in one place" rule as everything else.
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
    fn test_app_data_dir_lives_in_repo_library_folder_in_dev() {
        // Tests run with debug_assertions enabled, so app_data_dir()
        // resolves to <repo>/Library/. Verify the basename is "Library".
        let dir = app_data_dir();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some("Library"),
            "in dev/test, app_data_dir should be the repo's Library/ folder, got: {}",
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
