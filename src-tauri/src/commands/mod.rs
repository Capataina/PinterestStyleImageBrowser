//! Tauri command handlers, grouped by concern.
//!
//! Each submodule owns the `#[tauri::command]` functions for one
//! concern (images, tags, notes, roots, similarity, semantic,
//! profiling). `lib.rs::run()` registers all of them via
//! `tauri::generate_handler![...]` after re-importing them through
//! the `pub use` lines below.
//!
//! Two pieces of shared state live here rather than in any single
//! submodule because they're used across the similarity + semantic
//! commands:
//!
//! - `ImageSearchResult` — the unified return type for every
//!   cosine/semantic command (semantic_search, get_similar_images,
//!   get_tiered_similar_images). Single struct rather than a
//!   per-command shape so the frontend deserialises one type.
//! - `resolve_image_id_for_cosine_path` — maps a cosine-result path
//!   back to its DB `(id, canonical_path)`, with three lookup
//!   strategies for the various canonical-form mismatches.

use crate::db::{ImageDatabase, ID};
use crate::image_struct::ImageData;
use crate::paths;

pub mod encoders;
pub mod error;
pub mod images;
pub mod notes;
pub mod profiling;
pub mod roots;
pub mod semantic;
pub mod semantic_fused;
pub mod similarity;
pub mod tags;

pub use error::ApiError;

pub use images::*;
pub use notes::*;
pub use profiling::*;
pub use roots::*;
pub use semantic::*;
pub use similarity::*;
pub use tags::*;

/// Unified image-search result returned by every cosine/semantic
/// command (semantic_search, get_similar_images, get_tiered_similar_images).
///
/// Audit finding: `ImageSearchResult` and `ImageSearchResult` used to be
/// two near-identical structs — only difference was that the semantic
/// variant carried thumbnail enrichment. After the
/// "dimensions-to-backend" finding lands (this commit), all three
/// commands need the same fields, so they share one type. Field
/// shape preserved across both legacy struct names — this is a strict
/// superset of what `ImageSearchResult` used to send.
#[derive(serde::Serialize)]
pub struct ImageSearchResult {
    pub id: ID,
    pub path: String,
    pub score: f32,
    /// Absolute path to the thumbnail JPEG. None for legacy DB rows
    /// that pre-date the thumbnail migration; the frontend's
    /// `getThumbnailPath(id)` fallback covers this case.
    pub thumbnail_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Map a cosine-result `PathBuf` back to its database `(id, canonical_path)`.
///
/// The cosine index returns paths from its in-memory cache; those
/// paths might have a Windows extended-prefix (`\\?\`) if the
/// indexing pipeline canonicalised them on Windows, or a different
/// canonical form than what's stored in the DB. Three lookup
/// strategies, in order:
///
/// 1. Try the path with `\\?\` stripped (the common case — covers
///    every modern run on every platform).
/// 2. Fall back to the raw path the cosine index gave us.
/// 3. As a last resort, walk `all_images_cache` looking for any row
///    whose path matches under any normalisation. This handles
///    legacy DBs where some rows were inserted with one canonical
///    form and the cosine index now returns another.
///
/// Returns `Some((id, canonical_path))` if any strategy matches.
///
/// Audit finding (extracted from triplicated inline closures + 3
/// triplicated 60-line lookup blocks across `semantic_search`,
/// `get_similar_images`, `get_tiered_similar_images`). The project
/// notes already flagged "don't add a fourth normalisation closure"
/// — the third one was the redundancy.
pub(crate) fn resolve_image_id_for_cosine_path(
    db: &ImageDatabase,
    cosine_path: &std::path::Path,
    all_images_cache: Option<&[ImageData]>,
) -> Option<(ID, String)> {
    let path_str = cosine_path.to_string_lossy().into_owned();
    let normalized = paths::strip_windows_extended_prefix(&path_str).into_owned();

    // Strategy 1: direct DB lookup using the normalised path.
    if let Ok(id) = db.get_image_id_by_path(&normalized) {
        return Some((id, normalized));
    }
    // Strategy 2: direct DB lookup using the raw path.
    if let Ok(id) = db.get_image_id_by_path(&path_str) {
        return Some((id, path_str));
    }
    // Strategy 3: scan the cached image list for a flexible match.
    let images = all_images_cache?;
    let search_path = cosine_path
        .canonicalize()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| normalized.clone());

    images
        .iter()
        .find(|img| {
            let img_norm = paths::strip_windows_extended_prefix(&img.path);
            let img_canon = std::path::Path::new(&img.path)
                .canonicalize()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| img_norm.clone().into_owned());

            img_norm.as_ref() == normalized.as_str()
                || img_norm.as_ref() == path_str.as_str()
                || img.path == normalized
                || img.path == path_str
                || img_canon == search_path
        })
        .map(|img| (img.id, img.path.clone()))
}
