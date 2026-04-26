use tauri::State;
use tracing::{debug, info};

use crate::commands::{resolve_image_id_for_cosine_path, ApiError, ImageSearchResult};
use crate::db::ImageDatabase;
use crate::paths;
use crate::similarity_and_semantic_search::encoder_text::ClipTextEncoder;
use crate::{CosineIndexState, TextEncoderState};

/// Semantic search: find images matching a text query using CLIP embeddings
#[tauri::command]
#[tracing::instrument(name = "ipc.semantic_search", skip(db, cosine_state, text_encoder_state), fields(query_len = query.len(), top_n))]
pub fn semantic_search(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    text_encoder_state: State<'_, TextEncoderState>,
    query: String,
    top_n: usize,
) -> Result<Vec<ImageSearchResult>, ApiError> {
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
    let mut encoder_lock = text_encoder_state.encoder.lock()?;

    if encoder_lock.is_none() {
        info!("Initializing text encoder...");
        let models_dir = paths::models_dir();
        let model_path = models_dir.join("model_text.onnx");
        let tokenizer_path = models_dir.join("tokenizer.json");

        // Typed errors for the model-missing case so the frontend can
        // trigger a re-download dialog rather than showing a generic
        // toast (audit ApiError finding — discriminating the kind lets
        // the UI branch on the cause).
        if !model_path.exists() {
            return Err(ApiError::TextModelMissing(
                model_path.display().to_string(),
            ));
        }
        if !tokenizer_path.exists() {
            return Err(ApiError::TokenizerMissing(
                tokenizer_path.display().to_string(),
            ));
        }

        let encoder = ClipTextEncoder::new(&model_path, &tokenizer_path)
            .map_err(|e| ApiError::Encoder(format!("text encoder init failed: {e}")))?;

        *encoder_lock = Some(encoder);
        info!("Text encoder initialized successfully");
    }

    let encoder = encoder_lock.as_mut().unwrap();

    // Encode the text query
    debug!("Encoding query: '{}'", query);
    let text_embedding = encoder
        .encode(query)
        .map_err(|e| ApiError::Encoder(format!("encode query: {e}")))?;
    debug!(
        "Text embedding generated - length: {}",
        text_embedding.len()
    );

    // Ensure cosine index is populated
    let mut index = cosine_state.index.lock()?;

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

    // Convert results to ImageSearchResult with thumbnail info.
    // Path resolution + Windows-prefix normalisation is shared via
    // `resolve_image_id_for_cosine_path` (audit: extracted from the
    // triplicated normalize_path closure + 3-strategy lookup block).
    let results: Vec<ImageSearchResult> = raw_results
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
