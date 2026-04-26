use tauri::State;
use tracing::{debug, info, warn};

use crate::commands::{resolve_image_id_for_cosine_path, ApiError, ImageSearchResult};
use crate::db::ImageDatabase;
use crate::CosineIndexState;

#[tauri::command]
#[tracing::instrument(name = "ipc.get_tiered_similar_images", skip(db, cosine_state), fields(image_id, encoder_id = ?encoder_id))]
pub fn get_tiered_similar_images(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    image_id: i64,
    encoder_id: Option<String>,
) -> Result<Vec<ImageSearchResult>, ApiError> {
    use ndarray::Array1;
    use std::path::PathBuf;

    // Default to CLIP-ViT-B/32 if frontend hasn't migrated to passing
    // the param yet. After the picker UI ships, callers always pass
    // the user's selected image encoder ID.
    let encoder_id = encoder_id.unwrap_or_else(|| "clip_vit_b_32".to_string());

    info!(
        "get_tiered_similar_images - image_id: {} encoder: {}",
        image_id, encoder_id
    );

    // Ensure the cosine cache is loaded for the chosen encoder.
    // If the user just switched encoders in Settings, this triggers
    // a fast DB→memory transfer of the new encoder's embeddings.
    cosine_state
        .ensure_loaded_for(&db, &encoder_id)
        .map_err(|e| ApiError::Cosine(e))?;

    let mut index = cosine_state.index.lock()?;

    if index.cached_images.is_empty() {
        debug!("Cache empty even after ensure_loaded_for — encoder probably has no embeddings yet (run indexing).");
    }

    // Hoist db.get_all_images() to once per command (audit finding —
    // was called twice: once for the exclude-path lookup and once
    // again later for flexible match). One LEFT-JOIN-aggregate query
    // covers both purposes.
    let all_images = db.get_all_images()?;

    let exclude_path = all_images
        .iter()
        .find(|img| img.id == image_id)
        .map(|img| PathBuf::from(&img.path));

    // Read the chosen encoder's embedding for the clicked image.
    // Falls back to legacy `images.embedding` for the CLIP case
    // (where that column is the source of truth).
    let embedding = if encoder_id == "clip_vit_b_32" {
        db.get_image_embedding(image_id)?
    } else {
        db.get_embedding(image_id, &encoder_id)?
    };

    let query = Array1::from_vec(embedding);
    let raw_results = index.get_tiered_similar_images(&query, exclude_path.as_ref());

    // Path resolution + thumbnail enrichment. The dimensions used to
    // be fetched frontend-side via N parallel `getImageSize` DOM image
    // loads (audit Performance finding) — moved to backend here so
    // the result lands fully-populated in one IPC round-trip. Uses
    // the same `db.get_image_thumbnail_info` helper that
    // `semantic_search` already calls.
    let results: Vec<ImageSearchResult> = raw_results
        .into_iter()
        .filter_map(|(path, score)| {
            resolve_image_id_for_cosine_path(&db, &path, Some(&all_images)).map(
                |(id, final_path)| {
                    let (thumbnail_path, width, height) = db
                        .get_image_thumbnail_info(id)
                        .ok()
                        .flatten()
                        .map(|(tp, w, h)| (Some(tp), Some(w), Some(h)))
                        .unwrap_or((None, None, None));
                    ImageSearchResult {
                        id,
                        path: final_path,
                        score,
                        thumbnail_path,
                        width,
                        height,
                    }
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
#[tracing::instrument(name = "ipc.get_similar_images", skip(db, cosine_state), fields(image_id, top_n, encoder_id = ?encoder_id))]
pub fn get_similar_images(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    image_id: i64,
    top_n: usize,
    encoder_id: Option<String>,
) -> Result<Vec<ImageSearchResult>, ApiError> {
    use ndarray::Array1;
    use std::path::PathBuf;

    let encoder_id = encoder_id.unwrap_or_else(|| "clip_vit_b_32".to_string());

    info!(
        "get_similar_images - image_id: {}, top_n: {}, encoder: {}",
        image_id, top_n, encoder_id
    );

    cosine_state
        .ensure_loaded_for(&db, &encoder_id)
        .map_err(|e| ApiError::Cosine(e))?;

    let mut index = cosine_state.index.lock()?;

    // Hoist db.get_all_images() to once per command (audit finding —
    // was called twice: once for the exclude-path lookup and once again
    // later for flexible matching). Single LEFT-JOIN-aggregate covers
    // both. Surfacing the error here is the right call — silent
    // get_all_images failure on the second call previously degraded
    // results to "no flexible match" without the user knowing why.
    debug!("Looking up image path for image_id: {}", image_id);
    let all_images = db.get_all_images()?;
    debug!("Total images in database: {}", all_images.len());

    let exclude_path = all_images.iter().find(|img| img.id == image_id).map(|img| {
        debug!("Found image - id: {}, path: {}", img.id, img.path);
        PathBuf::from(&img.path)
    });
    if exclude_path.is_none() {
        warn!("Could not find image with id: {}", image_id);
    }

    debug!("Fetching embedding for image_id: {} via {}", image_id, encoder_id);
    let embedding = if encoder_id == "clip_vit_b_32" {
        db.get_image_embedding(image_id)?
    } else {
        db.get_embedding(image_id, &encoder_id)?
    };
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

    debug!("Converting results to ImageSearchResult structs...");

    // Path resolution shared via `resolve_image_id_for_cosine_path`
    // (audit: extracted from triplicated normalize_path closure +
    // 60-line lookup block).
    let results: Vec<ImageSearchResult> = raw_results
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
                // Enrich with thumbnail info — same pattern as
                // semantic_search and get_tiered_similar_images. Saves
                // the frontend N parallel `getImageSize` DOM image
                // loads (audit Performance finding).
                let (thumbnail_path, width, height) = db
                    .get_image_thumbnail_info(id)
                    .ok()
                    .flatten()
                    .map(|(tp, w, h)| (Some(tp), Some(w), Some(h)))
                    .unwrap_or((None, None, None));
                ImageSearchResult {
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
