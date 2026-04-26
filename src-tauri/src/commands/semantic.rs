use tauri::State;
use tracing::{debug, info};

use crate::perf;

use crate::commands::{resolve_image_id_for_cosine_path, ApiError, ImageSearchResult};
use crate::db::ImageDatabase;
use crate::paths;
use crate::similarity_and_semantic_search::encoder_siglip2::{
    Siglip2TextEncoder, SIGLIP2_TEXT_MODEL_FILENAME, SIGLIP2_TOKENIZER_FILENAME,
};
use crate::similarity_and_semantic_search::encoder_text::ClipTextEncoder;
use crate::{CosineIndexState, TextEncoderState};

/// Stable encoder ids for the text-side picker. Must match the values
/// in `commands::encoders::ENCODERS` and the frontend `imageEncoder` /
/// `textEncoder` localStorage entries.
pub const CLIP_TEXT_ENCODER_ID: &str = "clip_vit_b_32";
pub const SIGLIP2_TEXT_ENCODER_ID: &str = "siglip2_base";

/// Semantic search: find images matching a text query using the user's
/// chosen text encoder.
///
/// Phase 4 dispatch:
///
/// `text_encoder_id` — Optional encoder id from the frontend
/// `useUserPreferences().textEncoder` setting. Recognised values:
///   - `Some("siglip2_base")`  → SigLIP-2 768-d shared text+image space
///   - `Some("clip_vit_b_32")` → CLIP English 512-d (default)
///   - `None` or anything else → CLIP fallback
///
/// The cosine cache is loaded for the *matching* image-encoder family
/// (CLIP image embeddings if CLIP text was used; SigLIP-2 image
/// embeddings if SigLIP-2 text was used). Mixing dimensions would
/// crash with a dim-mismatch panic in ndarray's dot product — the
/// `ensure_loaded_for` call below guarantees the right cache is
/// resident before we touch it.
#[tauri::command]
#[tracing::instrument(name = "ipc.semantic_search", skip(db, cosine_state, text_encoder_state), fields(query_len = query.len(), top_n, text_encoder_id))]
pub fn semantic_search(
    db: State<'_, ImageDatabase>,
    cosine_state: State<'_, CosineIndexState>,
    text_encoder_state: State<'_, TextEncoderState>,
    query: String,
    top_n: usize,
    text_encoder_id: Option<String>,
) -> Result<Vec<ImageSearchResult>, ApiError> {
    use ndarray::Array1;

    let chosen = match text_encoder_id.as_deref() {
        Some(SIGLIP2_TEXT_ENCODER_ID) => SIGLIP2_TEXT_ENCODER_ID,
        // Default + explicit CLIP + any unknown id all fall through to CLIP
        // (the bullet-proof default). The frontend already validates ids
        // against list_available_encoders, but we don't trust that here.
        _ => CLIP_TEXT_ENCODER_ID,
    };

    info!(
        "semantic_search called - query: '{}', top_n: {}, encoder: {chosen}",
        query, top_n
    );

    // Validate query
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    // Branch on the chosen encoder. Each branch produces one Vec<f32>
    // text_embedding + a `dim` for the diagnostic + the cosine_cache_id
    // ("which image-side cache do we need to be loaded?").
    let (text_embedding, dim, cosine_cache_id) = if chosen == SIGLIP2_TEXT_ENCODER_ID {
        encode_with_siglip2(&text_encoder_state, query)?
    } else {
        encode_with_clip(&text_encoder_state, query)?
    };

    debug!(
        "Text embedding generated for {chosen} - length: {}",
        text_embedding.len()
    );

    // Force the cosine cache to hold image embeddings from the matching
    // encoder family. Without this, a previous "View Similar" call with
    // DINOv2 (768-d) selected would leave the cache as DINOv2 — a CLIP
    // text query (512-d) would crash the dot product on dim mismatch.
    cosine_state
        .ensure_loaded_for(&db, cosine_cache_id)
        .map_err(ApiError::Cosine)?;
    let mut index = cosine_state.index.lock()?;

    if index.cached_images.is_empty() {
        debug!("Populating cosine index from database (cache was empty)...");
        index.populate_from_db_for_encoder(&db, cosine_cache_id);
        debug!(
            "Cosine index populated with {} images",
            index.cached_images.len()
        );
    }

    // Find similar images using cosine similarity.
    let query_array = Array1::from_vec(text_embedding.clone());
    let cache_size = index.cached_images.len();
    let raw_results = index.get_similar_images_sorted(&query_array, top_n, None);
    let raw_scores: Vec<f32> = raw_results.iter().map(|(_, s)| *s).collect();
    debug!(
        "Found {} similar images for query '{}'",
        raw_results.len(),
        query
    );

    let all_images = db.get_all_images().ok();

    let mut resolution_misses: Vec<String> = Vec::new();
    let mut thumb_misses: u32 = 0;
    let results: Vec<ImageSearchResult> = raw_results
        .iter()
        .cloned()
        .filter_map(|(path, score)| {
            let image_info =
                resolve_image_id_for_cosine_path(&db, &path, all_images.as_deref());
            if image_info.is_none() {
                resolution_misses.push(path.to_string_lossy().into_owned());
            }
            image_info.map(|(id, final_path)| {
                let thumbnail_info = db.get_image_thumbnail_info(id).ok().flatten();
                if thumbnail_info.is_none() {
                    thumb_misses += 1;
                }
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

    // Query-embedding health stats.
    let q_norm: f32 = text_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    let q_nan_count = text_embedding.iter().filter(|x| x.is_nan()).count();
    let q_inf_count = text_embedding.iter().filter(|x| x.is_infinite()).count();
    let q_min = text_embedding.iter().cloned().fold(f32::INFINITY, f32::min);
    let q_max = text_embedding.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    perf::record_diagnostic(
        "search_query",
        serde_json::json!({
            "type": "semantic",
            "encoder_id": chosen,
            "top_n": top_n,
            "query_text": query,
            "cosine_cache_size": cache_size,
            "query_embedding": {
                "dim": dim,
                "l2_norm": q_norm,
                "min": q_min,
                "max": q_max,
                "nan_count": q_nan_count,
                "inf_count": q_inf_count,
                "interpretation": if q_nan_count > 0 || q_inf_count > 0 {
                    "BROKEN — NaN/Inf in query embedding (text encoder bug)"
                } else if (q_norm - 1.0).abs() < 0.01 {
                    "OK — normalised unit vector"
                } else if q_norm < 0.1 {
                    "WARNING — near-zero norm, encoder produced degenerate output"
                } else {
                    "Non-normalised — cosine still works since math divides by norms"
                },
            },
            "raw_results": raw_results.iter().map(|(p, s)| serde_json::json!({
                "path": p.to_string_lossy(),
                "score": *s,
            })).collect::<Vec<_>>(),
            "raw_result_count": raw_results.len(),
            "score_distribution":
                crate::similarity_and_semantic_search::cosine::diagnostics::score_distribution_stats(&raw_scores),
            "path_resolution_outcomes": {
                "raw_count": raw_results.len(),
                "resolved_count": results.len(),
                "missed_count": resolution_misses.len(),
                "thumbnail_misses": thumb_misses,
                "missed_paths_sample": resolution_misses.iter().take(10).cloned().collect::<Vec<_>>(),
            },
        }),
    );

    info!("semantic_search returning {} results ({chosen})", results.len());

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

/// CLIP text encode + tokenizer diagnostic. Returns
/// (embedding, dim, cosine_cache_id_to_load).
fn encode_with_clip(
    text_encoder_state: &TextEncoderState,
    query: &str,
) -> Result<(Vec<f32>, usize, &'static str), ApiError> {
    let mut encoder_lock = text_encoder_state.encoder.lock()?;

    if encoder_lock.is_none() {
        info!("Initializing CLIP text encoder...");
        let models_dir = paths::models_dir();
        let model_path = models_dir.join(crate::model_download::CLIP_TEXT_FILENAME);
        let tokenizer_path = models_dir.join(crate::model_download::CLIP_TOKENIZER_FILENAME);
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
            .map_err(|e| ApiError::Encoder(format!("CLIP text encoder init failed: {e}")))?;
        *encoder_lock = Some(encoder);
        info!("CLIP text encoder initialized successfully");
    }

    let encoder = encoder_lock.as_mut().unwrap();
    record_clip_tokenizer_diagnostic(encoder, query);
    let emb = encoder
        .encode(query)
        .map_err(|e| ApiError::Encoder(format!("CLIP encode query: {e}")))?;
    let dim = emb.len();
    Ok((emb, dim, CLIP_TEXT_ENCODER_ID))
}

/// SigLIP-2 text encode + diagnostic. Returns the same shape as
/// encode_with_clip so the call-site stays uniform.
fn encode_with_siglip2(
    text_encoder_state: &TextEncoderState,
    query: &str,
) -> Result<(Vec<f32>, usize, &'static str), ApiError> {
    let mut encoder_lock = text_encoder_state.siglip2_encoder.lock()?;

    if encoder_lock.is_none() {
        info!("Initializing SigLIP-2 text encoder...");
        let models_dir = paths::models_dir();
        let model_path = models_dir.join(SIGLIP2_TEXT_MODEL_FILENAME);
        let tokenizer_path = models_dir.join(SIGLIP2_TOKENIZER_FILENAME);
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
        let encoder = Siglip2TextEncoder::new(&model_path, &tokenizer_path)
            .map_err(|e| ApiError::Encoder(format!("SigLIP-2 text encoder init failed: {e}")))?;
        *encoder_lock = Some(encoder);
        info!("SigLIP-2 text encoder initialized successfully");
    }

    let encoder = encoder_lock.as_mut().unwrap();
    // SigLIP-2 doesn't expose the same tokenizer_for_diagnostic helper;
    // emit a minimal "encoder-id only" tokenizer_output so the report
    // still shows a row for this query and you can spot the encoder
    // change at a glance.
    perf::record_diagnostic(
        "tokenizer_output",
        serde_json::json!({
            "encoder_id": SIGLIP2_TEXT_ENCODER_ID,
            "raw_query": query,
            "raw_query_len_chars": query.chars().count(),
            "interpretation": "OK (SigLIP-2 SentencePiece — token ids not surfaced in diagnostic)",
        }),
    );
    use crate::similarity_and_semantic_search::encoders::TextEncoder as TextEncoderTrait;
    let emb = encoder
        .encode(query)
        .map_err(|e| ApiError::Encoder(format!("SigLIP-2 encode query: {e}")))?;
    let dim = emb.len();
    Ok((emb, dim, SIGLIP2_TEXT_ENCODER_ID))
}

/// CLIP-specific tokenizer diagnostic — same payload as before the
/// Phase 4 split. SigLIP-2 doesn't expose the equivalent shape
/// (different tokenizer, different vocab semantics).
fn record_clip_tokenizer_diagnostic(encoder: &ClipTextEncoder, query: &str) {
    let tok = encoder.tokenizer_for_diagnostic();
    match tok.encode(query, true) {
        Ok(encoding) => {
            let ids: Vec<u32> = encoding.get_ids().to_vec();
            let tokens: Vec<String> = encoding.get_tokens().to_vec();
            let attn: Vec<u32> = encoding.get_attention_mask().to_vec();
            let attn_sum: u32 = attn.iter().sum();
            let unk_count = tokens
                .iter()
                .filter(|t| t.contains("<unk>") || t.contains("[UNK]"))
                .count();
            perf::record_diagnostic(
                "tokenizer_output",
                serde_json::json!({
                    "encoder_id": CLIP_TEXT_ENCODER_ID,
                    "raw_query": query,
                    "raw_query_len_chars": query.chars().count(),
                    "token_count": ids.len(),
                    "attention_mask_sum": attn_sum,
                    "max_seq_length": encoder.max_seq_length(),
                    "token_ids": ids,
                    "decoded_tokens": tokens,
                    "interpretation": if ids.len() <= 2 {
                        "WARNING: only special tokens — query produced zero real tokens (empty query or tokenizer broken)"
                    } else if unk_count > ids.len() / 2 {
                        "WARNING: majority of tokens are <unk> — vocab mismatch with model"
                    } else if ids.len() > encoder.max_seq_length() {
                        "WARNING: query exceeds max_seq_length and will be truncated mid-content"
                    } else {
                        "OK"
                    },
                }),
            );
        }
        Err(e) => {
            perf::record_diagnostic(
                "tokenizer_output",
                serde_json::json!({
                    "encoder_id": CLIP_TEXT_ENCODER_ID,
                    "raw_query": query,
                    "error": e.to_string(),
                    "interpretation": "ERROR: tokenizer.encode() failed",
                }),
            );
        }
    }
}
