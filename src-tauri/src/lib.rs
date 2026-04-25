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
pub mod root_struct;
pub mod settings;
pub mod similarity_and_semantic_search;
pub mod tag_struct;
pub mod thumbnail;
pub mod watcher;

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
fn get_images(
    db: State<'_, ImageDatabase>,
    filter_tag_ids: Vec<ID>,
    filter_string: String,
) -> Result<Vec<ImageData>, String> {
    // Use the new method that includes thumbnail info
    return db
        .get_images_with_thumbnails(filter_tag_ids, filter_string)
        .map_err(|e| e.to_string());
}

#[tauri::command]
fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, String> {
    return db.get_tags().map_err(|e| e.to_string());
}

#[tauri::command]
fn create_tag(db: State<'_, ImageDatabase>, name: String, color: String) -> Result<Tag, String> {
    return db.create_tag(name, color).map_err(|e| e.to_string());
}

#[tauri::command]
fn delete_tag(db: State<'_, ImageDatabase>, tag_id: i64) -> Result<(), String> {
    db.delete_tag(tag_id).map_err(|e| e.to_string())
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
fn list_roots(db: State<'_, ImageDatabase>) -> Result<Vec<Root>, String> {
    db.list_roots().map_err(|e| e.to_string())
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
/// surviving image rows from other roots are unaffected.
#[tauri::command]
fn remove_root(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    id: i64,
) -> Result<(), String> {
    db.remove_root(id).map_err(|e| e.to_string())?;
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

    // Helper function to normalize Windows paths
    let normalize_path = |path_str: &str| -> String {
        if path_str.starts_with("\\\\?\\") {
            path_str[4..].to_string()
        } else {
            path_str.to_string()
        }
    };

    // Get all images for flexible path matching and thumbnail info
    let all_images = db.get_all_images().ok();

    // Convert results to SemanticSearchResult with thumbnail info
    let results: Vec<SemanticSearchResult> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            let path_str = path.to_string_lossy().to_string();
            let normalized_path = normalize_path(&path_str);

            // Try to find the image in the database
            let image_info = db
                .get_image_id_by_path(&normalized_path)
                .ok()
                .map(|id| (id, normalized_path.clone()))
                .or_else(|| {
                    db.get_image_id_by_path(&path_str)
                        .ok()
                        .map(|id| (id, path_str.clone()))
                })
                .or_else(|| {
                    // Flexible matching
                    all_images.as_ref().and_then(|images| {
                        images.iter().find(|img| {
                            let img_normalized = normalize_path(&img.path);
                            img_normalized == normalized_path
                                || img_normalized == path_str
                                || img.path == normalized_path
                                || img.path == path_str
                        }).map(|img| (img.id, img.path.clone()))
                    })
                });

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

    // Get the path of the clicked image to exclude it from results
    let exclude_path = {
        let all_images = db
            .get_all_images()
            .map_err(|e| format!("Failed to get images: {e}"))?;
        all_images
            .iter()
            .find(|img| img.id == image_id)
            .map(|img| PathBuf::from(&img.path))
    };

    let embedding = db
        .get_image_embedding(image_id)
        .map_err(|e| format!("Failed to fetch embedding: {e}"))?;

    let query = Array1::from_vec(embedding);
    let raw_results = index.get_tiered_similar_images(&query, exclude_path.as_ref());

    // Helper function to normalize path for database lookup
    let normalize_path = |path_str: &str| -> String {
        if path_str.starts_with("\\\\?\\") {
            path_str[4..].to_string()
        } else {
            path_str.to_string()
        }
    };

    let all_images = db.get_all_images().ok();

    let results: Vec<SimilarImage> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            let path_str = path.to_string_lossy().to_string();
            let normalized_path = normalize_path(&path_str);

            match db.get_image_id_by_path(&normalized_path) {
                Ok(id) => Some(SimilarImage {
                    id,
                    path: normalized_path,
                    score,
                }),
                Err(_) => match db.get_image_id_by_path(&path_str) {
                    Ok(id) => Some(SimilarImage {
                        id,
                        path: path_str,
                        score,
                    }),
                    Err(_) => {
                        if let Some(ref images) = all_images {
                            let search_path = path
                                .canonicalize()
                                .ok()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| normalize_path(&path_str));

                            images
                                .iter()
                                .find(|img| {
                                    let img_normalized = normalize_path(&img.path);
                                    let img_canon = std::path::Path::new(&img.path)
                                        .canonicalize()
                                        .ok()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|| normalize_path(&img.path));

                                    img_normalized == normalized_path
                                        || img_normalized == path_str
                                        || img.path == normalized_path
                                        || img.path == path_str
                                        || img_canon == search_path
                                })
                                .map(|matching_image| SimilarImage {
                                    id: matching_image.id,
                                    path: matching_image.path.clone(),
                                    score,
                                })
                        } else {
                            None
                        }
                    }
                },
            }
        })
        .collect();

    info!(
        "get_tiered_similar_images returning {} results",
        results.len()
    );

    Ok(results)
}

#[tauri::command]
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

    // Get the path of the clicked image to exclude it from results
    debug!("Looking up image path for image_id: {}", image_id);
    let exclude_path = {
        let all_images = db
            .get_all_images()
            .map_err(|e| format!("Failed to get images: {e}"))?;
        debug!("Total images in database: {}", all_images.len());
        let found = all_images.iter().find(|img| img.id == image_id).map(|img| {
            debug!("Found image - id: {}, path: {}", img.id, img.path);
            PathBuf::from(&img.path)
        });
        if found.is_none() {
            warn!(
                "Could not find image with id: {}",
                image_id
            );
        }
        found
    };

    debug!("Fetching embedding for image_id: {}", image_id);
    let embedding = db
        .get_image_embedding(image_id)
        .map_err(|e| format!("Failed to fetch embedding: {e}"))?;
    debug!(
        "Retrieved embedding - length: {}",
        embedding.len()
    );

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
    
    // Helper function to normalize path for database lookup
    // Removes Windows extended path prefix (\\?\) if present
    let normalize_path = |path_str: &str| -> String {
        if path_str.starts_with("\\\\?\\") {
            path_str[4..].to_string()
        } else {
            path_str.to_string()
        }
    };
    
    // Get all images once for flexible matching if needed
    let all_images = match db.get_all_images() {
        Ok(images) => Some(images),
        Err(e) => {
            warn!("Failed to get all images for flexible matching: {}", e);
            None
        }
    };
    
    let results: Vec<SimilarImage> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            let path_str = path.to_string_lossy().to_string();
            let normalized_path = normalize_path(&path_str);
            
            // Try normalized path first (most common case)
            match db.get_image_id_by_path(&normalized_path) {
                Ok(id) => {
                    debug!(
                        "  Mapped path to id - path: {:?}, id: {}, score: {:.4}",
                        path.file_name().unwrap_or_default(),
                        id,
                        score
                    );
                    Some(SimilarImage {
                        id,
                        path: normalized_path,
                        score,
                    })
                }
                Err(_) => {
                    // Try original path format
                    match db.get_image_id_by_path(&path_str) {
                        Ok(id) => {
                            debug!(
                                "  Mapped path to id (original format) - path: {:?}, id: {}, score: {:.4}",
                                path.file_name().unwrap_or_default(),
                                id,
                                score
                            );
                            Some(SimilarImage {
                                id,
                                path: path_str,
                                score,
                            })
                        }
                        Err(_) => {
                            // Fallback: flexible matching by comparing canonicalized paths
                            if let Some(ref images) = all_images {
                                let search_path = path.canonicalize()
                                    .ok()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|| normalize_path(&path_str));

                                if let Some(matching_image) = images.iter().find(|img| {
                                    let img_normalized = normalize_path(&img.path);
                                    let img_canon = std::path::Path::new(&img.path)
                                        .canonicalize()
                                        .ok()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|| normalize_path(&img.path));

                                    img_normalized == normalized_path ||
                                    img_normalized == path_str ||
                                    img.path == normalized_path ||
                                    img.path == path_str ||
                                    img_canon == search_path
                                }) {
                                    debug!(
                                        "  Mapped path to id (flexible match) - path: {:?}, id: {}, score: {:.4}",
                                        path.file_name().unwrap_or_default(),
                                        matching_image.id,
                                        score
                                    );
                                    Some(SimilarImage {
                                        id: matching_image.id,
                                        path: matching_image.path.clone(),
                                        score,
                                    })
                                } else {
                                    warn!(
                                        "  Failed to map path to id - path: {:?}",
                                        path.file_name().unwrap_or_default()
                                    );
                                    None
                                }
                            } else {
                                warn!(
                                    "  Failed to map path to id - path: {:?}",
                                    path.file_name().unwrap_or_default()
                                );
                                None
                            }
                        }
                    }
                }
            }
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
