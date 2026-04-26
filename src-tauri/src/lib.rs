use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tracing::{debug, error, info, warn};

use crate::{
    db::{ImageDatabase, ID},
    image_struct::ImageData,
    indexing::IndexingState,
    root_struct::Root,
    similarity_and_semantic_search::cosine_similarity::CosineIndex,
    similarity_and_semantic_search::encoder_text::TextEncoder,
    tag_struct::Tag,
};

#[derive(serde::Serialize)]
struct SimilarImage {
    id: ID,
    path: String,
    score: f32,
}

#[derive(serde::Serialize)]
struct SemanticSearchResult {
    id: ID,
    path: String,
    score: f32,
    thumbnail_path: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod db;
pub mod filesystem;
pub mod image_struct;
pub mod indexing;
pub mod model_download;
pub mod paths;
pub mod perf;
pub mod perf_report;
pub mod root_struct;
pub mod settings;
pub mod similarity_and_semantic_search;
pub mod tag_struct;
pub mod thumbnail;
pub mod watcher;

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
fn resolve_image_id_for_cosine_path(
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

pub struct CosineIndexState {
    /// Wrapped in Arc<Mutex<...>> rather than plain Mutex<...> so the
    /// indexing thread (Pass 5) can hold a clone alongside the Tauri-
    /// managed state. Both point at the same in-memory cache.
    pub index: Arc<Mutex<CosineIndex>>,
    pub db_path: String,
}

/// State for the text encoder used in semantic search
/// Lazy-loaded on first semantic search query
pub struct TextEncoderState {
    pub encoder: Mutex<Option<TextEncoder>>,
}

#[tauri::command]
#[tracing::instrument(name = "ipc.get_images", skip(db), fields(tag_count = filter_tag_ids.len()))]
fn get_images(
    db: State<'_, ImageDatabase>,
    filter_tag_ids: Vec<ID>,
    filter_string: String,
    match_all_tags: Option<bool>,
) -> Result<Vec<ImageData>, String> {
    // match_all_tags is Option so older frontend builds (or tests)
    // can call without specifying — defaults to false (OR semantic).
    let match_all = match_all_tags.unwrap_or(false);
    db.get_images_with_thumbnails(filter_tag_ids, filter_string, match_all)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[tracing::instrument(name = "ipc.get_tags", skip(db))]
fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, String> {
    return db.get_tags().map_err(|e| e.to_string());
}

#[tauri::command]
#[tracing::instrument(name = "ipc.create_tag", skip(db))]
fn create_tag(db: State<'_, ImageDatabase>, name: String, color: String) -> Result<Tag, String> {
    return db.create_tag(name, color).map_err(|e| e.to_string());
}

#[tauri::command]
fn delete_tag(db: State<'_, ImageDatabase>, tag_id: i64) -> Result<(), String> {
    db.delete_tag(tag_id).map_err(|e| e.to_string())
}

/// Read the free-text annotation for an image. Returns "" if there
/// is no annotation set (the column is either NULL or "" — we treat
/// both as "no annotation" at the user-facing level).
#[tauri::command]
fn get_image_notes(db: State<'_, ImageDatabase>, image_id: i64) -> Result<String, String> {
    db.get_image_notes(image_id)
        .map(|opt| opt.unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Write an annotation for an image. Empty / whitespace-only string
/// clears the field.
#[tauri::command]
fn set_image_notes(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    notes: String,
) -> Result<(), String> {
    db.set_image_notes(image_id, &notes)
        .map_err(|e| e.to_string())
}

/// Read the currently-configured scan root from settings.json, if any.
/// Returns Ok(None) when no root has been picked yet (first-launch state).
#[tauri::command]
fn get_scan_root() -> Result<Option<String>, String> {
    Ok(settings::Settings::load()
        .scan_root
        .map(|p| p.to_string_lossy().into_owned()))
}

/// Replace every configured root with a single new one and trigger a
/// live re-index. This is what the "Choose folder" button calls — the
/// "I just want one folder, replace what's there" UX. For multi-folder
/// management see `add_root` / `remove_root` / `set_root_enabled`.
///
/// The tag catalogue is preserved across root replacement.
#[tauri::command]
fn set_scan_root(
    app: AppHandle,
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    indexing_state: State<'_, Arc<IndexingState>>,
    path: String,
) -> Result<(), String> {
    let scan_root = PathBuf::from(&path);
    if !scan_root.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    // Remove existing roots (CASCADE deletes their images), wipe any
    // orphan rows (NULL root_id from older DBs), then add the new one.
    let existing = db.list_roots().map_err(|e| e.to_string())?;
    for r in existing {
        db.remove_root(r.id).map_err(|e| e.to_string())?;
    }
    db.wipe_images_for_new_root()
        .map_err(|e| format!("Failed to wipe legacy NULL-root_id rows: {e}"))?;

    db.add_root(path.clone())
        .map_err(|e| format!("Failed to add root: {e}"))?;

    if let Ok(mut idx) = cosine_state.index.lock() {
        idx.cached_images.clear();
    }

    indexing::try_spawn_pipeline(
        app.clone(),
        indexing_state.inner().clone(),
        cosine_state.db_path.clone(),
        cosine_state.index.clone(),
    )
    .map_err(|e| e.to_string())?;

    info!("set_scan_root replaced roots + spawned indexing.");
    Ok(())
}

/// Multi-folder management — list configured roots.
#[tauri::command]
#[tracing::instrument(name = "ipc.list_roots", skip(db))]
fn list_roots(db: State<'_, ImageDatabase>) -> Result<Vec<Root>, String> {
    db.list_roots().map_err(|e| e.to_string())
}

/// True if the binary was launched with `--profile`. The frontend
/// reads this once at startup to decide whether to mount the perf
/// overlay, register the cmd+shift+P shortcut, and emit user-action
/// breadcrumbs. Without the flag, all of those paths stay dormant
/// and the app pays no profiling cost.
#[tauri::command]
fn is_profiling_enabled() -> bool {
    perf::is_profiling_enabled()
}

/// Returns aggregated span timing stats for the in-app perf overlay.
/// Frontend polls this to render the live diagnostics panel.
#[tauri::command]
fn get_perf_snapshot() -> perf::PerfSnapshot {
    perf::snapshot()
}

/// Wipe collected perf stats. Useful between scenarios when measuring
/// a specific operation in isolation.
#[tauri::command]
fn reset_perf_stats() -> Result<(), String> {
    perf::reset();
    Ok(())
}

/// Append a user action to the profiling timeline. No-op when the
/// app isn't in profiling mode (the frontend checks this before
/// calling, but we double-check on the backend so a stale
/// profilingCache can't poison the timeline).
///
/// Payload is free-form JSON — call sites attach whatever's relevant
/// (query text, image id, tag id, sort mode...). The on-exit
/// markdown renderer correlates these with span events that fired
/// in the next ~500ms.
#[tauri::command]
fn record_user_action(action: String, payload: serde_json::Value) {
    perf::record_user_action(action, payload);
}

/// Write the current perf snapshot to Library/exports/perf-<unix-ts>.json
/// as pretty-printed JSON. Returns the absolute path so the frontend
/// can show it in a confirmation message.
#[tauri::command]
fn export_perf_snapshot() -> Result<String, String> {
    let snap = perf::snapshot();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = paths::exports_dir().join(format!("perf-{now}.json"));
    let json = serde_json::to_string_pretty(&snap)
        .map_err(|e| format!("Failed to serialise snapshot: {e}"))?;
    std::fs::write(&dest, json)
        .map_err(|e| format!("Failed to write export: {e}"))?;
    Ok(dest.to_string_lossy().into_owned())
}

/// Add a root and trigger an incremental re-index. Returns the new
/// Root row so the UI can show it immediately without round-tripping
/// list_roots.
#[tauri::command]
fn add_root(
    app: AppHandle,
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    indexing_state: State<'_, Arc<IndexingState>>,
    path: String,
) -> Result<Root, String> {
    let scan_root = PathBuf::from(&path);
    if !scan_root.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    let root = db
        .add_root(path)
        .map_err(|e| format!("Failed to add root: {e}"))?;

    indexing::try_spawn_pipeline(
        app.clone(),
        indexing_state.inner().clone(),
        cosine_state.db_path.clone(),
        cosine_state.index.clone(),
    )
    .map_err(|e| e.to_string())?;

    info!("add_root persisted ({}) and spawned re-index.", root.path);
    Ok(root)
}

/// Remove a root. The CASCADE on images.root_id wipes its images;
/// surviving image rows from other roots are unaffected. The root's
/// dedicated thumbnail directory on disk is also recursively
/// deleted so we don't leave orphaned cached files.
#[tauri::command]
fn remove_root(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    id: i64,
) -> Result<(), String> {
    db.remove_root(id).map_err(|e| e.to_string())?;
    // Clean the per-root thumbnail subfolder. Best-effort — if the
    // remove fails (permissions, file locked) we log and move on; the
    // user can manually clean the directory.
    let thumbnail_dir = paths::thumbnails_dir_for_root(id);
    if thumbnail_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&thumbnail_dir) {
            warn!(
                "could not remove thumbnail dir {}: {e}",
                thumbnail_dir.display()
            );
        } else {
            info!("removed thumbnail dir {}", thumbnail_dir.display());
        }
    }
    // Cosine cache contains entries from the removed root; cheapest
    // way to clean is to drop the whole cache and let next-query
    // populate from the remaining DB rows.
    if let Ok(mut idx) = cosine_state.index.lock() {
        idx.cached_images.clear();
    }
    info!("remove_root removed root id {}", id);
    Ok(())
}

/// Toggle a root's enabled flag. No re-index needed — the grid query
/// filters by enabled status, so the toggle is instant.
#[tauri::command]
fn set_root_enabled(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    db.set_root_enabled(id, enabled).map_err(|e| e.to_string())?;
    // Cosine cache may include images from the toggled root; clear so
    // the next similarity query rebuilds with the right active set.
    if let Ok(mut idx) = cosine_state.index.lock() {
        idx.cached_images.clear();
    }
    Ok(())
}

#[tauri::command]
fn add_tag_to_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), String> {
    db.add_tag_to_image(image_id, tag_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_tag_from_image(
    db: State<'_, ImageDatabase>,
    image_id: i64,
    tag_id: i64,
) -> Result<(), String> {
    db.remove_tag_from_image(image_id, tag_id)
        .map_err(|e| e.to_string())
}

/// Semantic search: find images matching a text query using CLIP embeddings
#[tauri::command]
#[tracing::instrument(name = "ipc.semantic_search", skip(db, cosine_state, text_encoder_state), fields(query_len = query.len(), top_n))]
fn semantic_search(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    text_encoder_state: State<'_, TextEncoderState>,
    query: String,
    top_n: usize,
) -> Result<Vec<SemanticSearchResult>, String> {
    use ndarray::Array1;

    info!(
        "semantic_search called - query: '{}', top_n: {}",
        query, top_n
    );

    // Validate query
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    // Lazy-load text encoder if not initialized
    let mut encoder_lock = text_encoder_state
        .encoder
        .lock()
        .map_err(|e| format!("Failed to lock text encoder: {e}"))?;

    if encoder_lock.is_none() {
        info!("Initializing text encoder...");
        let models_dir = paths::models_dir();
        let model_path = models_dir.join("model_text.onnx");
        let tokenizer_path = models_dir.join("tokenizer.json");

        if !model_path.exists() {
            return Err(format!(
                "Text model not found at: {}",
                model_path.display()
            ));
        }
        if !tokenizer_path.exists() {
            return Err(format!(
                "Tokenizer not found at: {}",
                tokenizer_path.display()
            ));
        }

        let encoder = TextEncoder::new(&model_path, &tokenizer_path)
            .map_err(|e| format!("Failed to initialize text encoder: {e}"))?;

        *encoder_lock = Some(encoder);
        info!("Text encoder initialized successfully");
    }

    let encoder = encoder_lock.as_mut().unwrap();

    // Encode the text query
    debug!("Encoding query: '{}'", query);
    let text_embedding = encoder
        .encode(query)
        .map_err(|e| format!("Failed to encode query: {e}"))?;
    debug!(
        "Text embedding generated - length: {}",
        text_embedding.len()
    );

    // Ensure cosine index is populated
    let mut index = cosine_state
        .index
        .lock()
        .map_err(|e| format!("Failed to lock cosine index: {e}"))?;

    if index.cached_images.is_empty() {
        debug!("Populating cosine index from database...");
        index.populate_from_db(&db);
        debug!(
            "Cosine index populated with {} images",
            index.cached_images.len()
        );
    }

    // Find similar images using cosine similarity
    // Use get_similar_images_sorted for semantic search to get results in proper order
    let query_array = Array1::from_vec(text_embedding);
    let raw_results = index.get_similar_images_sorted(&query_array, top_n, None);
    debug!(
        "Found {} similar images for query '{}'",
        raw_results.len(),
        query
    );

    // Get all images once for flexible matching (audit: was previously
    // implicit in semantic_search but two of the similarity commands
    // called get_all_images twice — hoisting once is the consistent
    // pattern across all three).
    let all_images = db.get_all_images().ok();

    // Convert results to SemanticSearchResult with thumbnail info.
    // Path resolution + Windows-prefix normalisation is shared via
    // `resolve_image_id_for_cosine_path` (audit: extracted from the
    // triplicated normalize_path closure + 3-strategy lookup block).
    let results: Vec<SemanticSearchResult> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            let image_info =
                resolve_image_id_for_cosine_path(&db, &path, all_images.as_deref());

            image_info.map(|(id, final_path)| {
                // Get thumbnail info if available
                let thumbnail_info = db.get_image_thumbnail_info(id).ok().flatten();
                let (thumbnail_path, width, height) = thumbnail_info
                    .map(|(tp, w, h)| (Some(tp), Some(w), Some(h)))
                    .unwrap_or((None, None, None));

                SemanticSearchResult {
                    id,
                    path: final_path,
                    score,
                    thumbnail_path,
                    width,
                    height,
                }
            })
        })
        .collect();

    info!(
        "semantic_search returning {} results",
        results.len()
    );

    if !results.is_empty() {
        debug!("Top 5 results:");
        for (i, r) in results.iter().take(5).enumerate() {
            debug!(
                "  {}. id: {}, score: {:.4}, path: {}",
                i + 1,
                r.id,
                r.score,
                std::path::Path::new(&r.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        }
    }

    Ok(results)
}

#[tauri::command]
#[tracing::instrument(name = "ipc.get_tiered_similar_images", skip(db, cosine_state), fields(image_id))]
fn get_tiered_similar_images(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    image_id: i64,
) -> Result<Vec<SimilarImage>, String> {
    use ndarray::Array1;
    use std::path::PathBuf;

    info!(
        "get_tiered_similar_images called - image_id: {}",
        image_id
    );

    let mut index = cosine_state
        .index
        .lock()
        .map_err(|e| format!("Failed to lock cosine index: {e}"))?;

    if index.cached_images.is_empty() {
        debug!("Cache is empty, populating from database...");
        index.populate_from_db(&db);
    }

    // Hoist db.get_all_images() to once per command (audit finding —
    // was called twice: once for the exclude-path lookup and once
    // again later for flexible match). One LEFT-JOIN-aggregate query
    // covers both purposes.
    let all_images = db
        .get_all_images()
        .map_err(|e| format!("Failed to get images: {e}"))?;

    let exclude_path = all_images
        .iter()
        .find(|img| img.id == image_id)
        .map(|img| PathBuf::from(&img.path));

    let embedding = db
        .get_image_embedding(image_id)
        .map_err(|e| format!("Failed to fetch embedding: {e}"))?;

    let query = Array1::from_vec(embedding);
    let raw_results = index.get_tiered_similar_images(&query, exclude_path.as_ref());

    // Path resolution shared via `resolve_image_id_for_cosine_path`.
    let results: Vec<SimilarImage> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            resolve_image_id_for_cosine_path(&db, &path, Some(&all_images)).map(
                |(id, final_path)| SimilarImage {
                    id,
                    path: final_path,
                    score,
                },
            )
        })
        .collect();

    info!(
        "get_tiered_similar_images returning {} results",
        results.len()
    );

    Ok(results)
}

#[tauri::command]
#[tracing::instrument(name = "ipc.get_similar_images", skip(db, cosine_state), fields(image_id, top_n))]
fn get_similar_images(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    image_id: i64,
    top_n: usize,
) -> Result<Vec<SimilarImage>, String> {
    use ndarray::Array1;
    use std::path::PathBuf;

    info!(
        "get_similar_images called - image_id: {}, top_n: {}",
        image_id, top_n
    );

    let mut index = cosine_state
        .index
        .lock()
        .map_err(|e| format!("Failed to lock cosine index: {e}"))?;

    debug!(
        "Cache state - cached_images count: {}",
        index.cached_images.len()
    );

    if index.cached_images.is_empty() {
        debug!("Cache is empty, populating from database...");
        index.populate_from_db(&db);
        debug!(
            "Cache populated - cached_images count: {}",
            index.cached_images.len()
        );
    }

    // Hoist db.get_all_images() to once per command (audit finding —
    // was called twice: once for the exclude-path lookup and once again
    // later for flexible matching). Single LEFT-JOIN-aggregate covers
    // both. Surfacing the error here is the right call — silent
    // get_all_images failure on the second call previously degraded
    // results to "no flexible match" without the user knowing why.
    debug!("Looking up image path for image_id: {}", image_id);
    let all_images = db
        .get_all_images()
        .map_err(|e| format!("Failed to get images: {e}"))?;
    debug!("Total images in database: {}", all_images.len());

    let exclude_path = all_images.iter().find(|img| img.id == image_id).map(|img| {
        debug!("Found image - id: {}, path: {}", img.id, img.path);
        PathBuf::from(&img.path)
    });
    if exclude_path.is_none() {
        warn!("Could not find image with id: {}", image_id);
    }

    debug!("Fetching embedding for image_id: {}", image_id);
    let embedding = db
        .get_image_embedding(image_id)
        .map_err(|e| format!("Failed to fetch embedding: {e}"))?;
    debug!("Retrieved embedding - length: {}", embedding.len());

    let query = Array1::from_vec(embedding);
    debug!(
        "Calling index.get_similar_images with top_n: {}, exclude_path: {:?}",
        top_n, exclude_path
    );
    let raw_results = index.get_similar_images(&query, top_n, exclude_path.as_ref());
    debug!(
        "index.get_similar_images returned {} results",
        raw_results.len()
    );

    if !raw_results.is_empty() {
        debug!("Raw results (first 5):");
        for (i, (path, score)) in raw_results.iter().take(5).enumerate() {
            debug!("  {}. path: {:?}, score: {:.4}", i + 1, path, score);
        }
    }

    debug!("Converting results to SimilarImage structs...");

    // Path resolution shared via `resolve_image_id_for_cosine_path`
    // (audit: extracted from triplicated normalize_path closure +
    // 60-line lookup block).
    let results: Vec<SimilarImage> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            let info = resolve_image_id_for_cosine_path(&db, &path, Some(&all_images));
            if info.is_none() {
                warn!(
                    "  Failed to map path to id - path: {:?}",
                    path.file_name().unwrap_or_default()
                );
            }
            info.map(|(id, final_path)| {
                debug!(
                    "  Mapped path to id - path: {:?}, id: {}, score: {:.4}",
                    path.file_name().unwrap_or_default(),
                    id,
                    score
                );
                SimilarImage {
                    id,
                    path: final_path,
                    score,
                }
            })
        })
        .collect();

    info!("Final results count: {}", results.len());
    if !results.is_empty() {
        debug!("Final results (first 5):");
        for (i, sim) in results.iter().take(5).enumerate() {
            debug!(
                "  {}. id: {}, path: {:?}, score: {:.4}",
                i + 1,
                sim.id,
                std::path::Path::new(&sim.path)
                    .file_name()
                    .unwrap_or_default(),
                sim.score
            );
        }
    }

    Ok(results)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(db: ImageDatabase, db_path: String) {
    let cosine_index = Arc::new(Mutex::new(CosineIndex::new()));
    let cosine_state = CosineIndexState {
        index: cosine_index.clone(),
        db_path: db_path.clone(),
    };

    // Text encoder state (lazy-loaded on first semantic search).
    let text_encoder_state = TextEncoderState {
        encoder: Mutex::new(None),
    };

    // Single-flight guard for the indexing pipeline. Wrapped in Arc so
    // the .setup() callback (and later set_scan_root commands) can both
    // hand a clone to the indexing thread.
    let indexing_state = Arc::new(IndexingState::new());

    // Filesystem watcher handle is stashed here so it lives for the
    // duration of the app process. Dropping the handle cancels every
    // watch; we wrap in Mutex<Option<...>> so the setup callback can
    // initialise it (or replace it later if root list changes).
    let watcher_state: Arc<Mutex<Option<watcher::WatcherHandle>>> =
        Arc::new(Mutex::new(None));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(db)
        .manage(cosine_state)
        .manage(text_encoder_state)
        .manage(indexing_state.clone())
        .manage(watcher_state.clone())
        .setup({
            let db_path = db_path.clone();
            let cosine_index = cosine_index.clone();
            let indexing_state = indexing_state.clone();
            let watcher_state = watcher_state.clone();
            move |app| {
                // One-shot legacy migration: if the user upgraded from
                // a single-folder build, settings.json has a `scan_root`
                // field but the new `roots` table is empty. Convert it
                // here so the indexing pipeline (which only reads roots
                // table) sees the user's existing folder.
                {
                    let user_settings = settings::Settings::load();
                    if let Some(legacy_path) = user_settings.scan_root.clone() {
                        if let Ok(temp_db) = ImageDatabase::new(&db_path) {
                            let _ = temp_db.initialize();
                            match temp_db.migrate_legacy_scan_root(
                                legacy_path.to_string_lossy().into_owned(),
                            ) {
                                Ok(Some(root)) => {
                                    info!(
                                        "migrated legacy scan_root -> roots[{}] ({})",
                                        root.id, root.path
                                    );
                                    // Clear the legacy field so we don't
                                    // re-migrate on every launch.
                                    let mut s = user_settings.clone();
                                    s.scan_root = None;
                                    let _ = s.save();
                                }
                                Ok(None) => {} // already migrated previously
                                Err(e) => warn!("legacy migration failed: {e}"),
                            }
                        }
                    }
                }

                // Auto-spawn the indexing pipeline at app startup. This
                // refreshes the catalog whenever the user reopens the
                // app — picks up new images, regenerates missing
                // thumbnails, encodes anything missing.
                let app_handle = app.handle().clone();
                if let Err(e) = indexing::try_spawn_pipeline(
                    app_handle.clone(),
                    indexing_state.clone(),
                    db_path.clone(),
                    cosine_index.clone(),
                ) {
                    error!("could not spawn indexing pipeline: {e}");
                }

                // Start the filesystem watcher. Listens to every
                // currently-enabled root and triggers a debounced
                // rescan when files change on disk.
                {
                    let temp_db = ImageDatabase::new(&db_path);
                    let watch_paths: Vec<std::path::PathBuf> = match temp_db {
                        Ok(d) => d
                            .list_roots()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|r| r.enabled)
                            .map(|r| std::path::PathBuf::from(r.path))
                            .filter(|p| p.exists())
                            .collect(),
                        Err(_) => vec![],
                    };
                    let handle = watcher::start(
                        app_handle,
                        watch_paths,
                        db_path,
                        indexing_state,
                        cosine_index,
                    );
                    if let Ok(mut slot) = watcher_state.lock() {
                        *slot = handle;
                    }
                }
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_images,
            get_tags,
            create_tag,
            delete_tag,
            add_tag_to_image,
            remove_tag_from_image,
            get_similar_images,
            get_tiered_similar_images,
            semantic_search,
            get_scan_root,
            set_scan_root,
            list_roots,
            add_root,
            remove_root,
            set_root_enabled,
            get_image_notes,
            set_image_notes,
            is_profiling_enabled,
            get_perf_snapshot,
            reset_perf_stats,
            export_perf_snapshot,
            record_user_action,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // RunEvent::Exit fires when the last window closes and the
            // app is genuinely shutting down. This is our last chance
            // to render the markdown report from the timeline.jsonl
            // that's been accumulating during the session.
            //
            // We deliberately don't render on ExitRequested — that
            // fires before windows are torn down and could be cancelled.
            // Exit is the point of no return, and the flush thread is
            // about to die with the process anyway.
            //
            // No-op when profiling isn't enabled (no session dir, no
            // timeline file, nothing to report).
            if let tauri::RunEvent::Exit = event {
                if perf::is_profiling_enabled() {
                    if let Some(dir) = perf::session_dir() {
                        match perf_report::render_session_report(&dir) {
                            Ok(_) => {
                                // Use eprintln rather than tracing here
                                // — the subscriber may already be tearing
                                // down at exit, and we want this line to
                                // make it to the terminal regardless.
                                eprintln!(
                                    "profiling report written to {}",
                                    dir.display()
                                );
                            }
                            Err(e) => {
                                eprintln!("failed to write profiling report: {e}");
                            }
                        }
                    }
                }
            }
        });
}
