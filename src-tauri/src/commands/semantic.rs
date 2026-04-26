use tauri::State;
use tracing::{debug, info};

use crate::perf;

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
        // New filenames — separate-graph CLIP text model, BPE tokenizer.
        // The old `model_text.onnx` (multilingual distillation) is now
        // unused; if it exists from a previous install, it sits idle
        // in models_dir until cleanup is added.
        let model_path = models_dir.join(crate::model_download::CLIP_TEXT_FILENAME);
        let tokenizer_path = models_dir.join(crate::model_download::CLIP_TOKENIZER_FILENAME);

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

    // Tokenizer diagnostic — emit the raw query, decoded tokens, and
    // attention mask sum BEFORE running inference. Lets the report
    // show "user typed 'blue fish' → tokens ['<|startoftext|>', 'blue</w>',
    // 'fish</w>', '<|endoftext|>'], 4 real tokens out of 77 max" so a
    // user can spot tokenizer breakage (everything → <unk>, query
    // truncated mid-word, vocab mismatch with model).
    {
        let tok = encoder.tokenizer_for_diagnostic();
        match tok.encode(query, true) {
            Ok(encoding) => {
                let ids: Vec<u32> = encoding.get_ids().to_vec();
                let tokens: Vec<String> = encoding.get_tokens().to_vec();
                let attn: Vec<u32> = encoding.get_attention_mask().to_vec();
                let attn_sum: u32 = attn.iter().sum();
                let unk_count = tokens.iter().filter(|t| t.contains("<unk>") || t.contains("[UNK]")).count();
                perf::record_diagnostic(
                    "tokenizer_output",
                    serde_json::json!({
                        "encoder_id": "clip_vit_b_32",
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
                        "encoder_id": "clip_vit_b_32",
                        "raw_query": query,
                        "error": e.to_string(),
                        "interpretation": "ERROR: tokenizer.encode() failed",
                    }),
                );
            }
        }
    }

    // Encode the text query
    debug!("Encoding query: '{}'", query);
    let text_embedding = encoder
        .encode(query)
        .map_err(|e| ApiError::Encoder(format!("encode query: {e}")))?;
    debug!(
        "Text embedding generated - length: {}",
        text_embedding.len()
    );

    // Force the cosine cache to hold CLIP-family image embeddings —
    // we use ClipTextEncoder above, which produces 512-d vectors that
    // only make sense compared against CLIP-family image embeddings
    // (also 512-d). Without this, a previous "View Similar" call with
    // DINOv2 (384-d) selected would have left the cache in a state
    // where dot-producting against the 512-d text query crashes
    // ndarray with a dim-mismatch panic.
    //
    // Future: when SigLIP-2 text dispatch is wired, this should
    // pick the encoder matching the user's textEncoder setting.
    cosine_state
        .ensure_loaded_for(&db, "clip_vit_b_32")
        .map_err(|e| ApiError::Cosine(e))?;
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
    let query_array = Array1::from_vec(text_embedding.clone());
    let cache_size = index.cached_images.len();
    let raw_results = index.get_similar_images_sorted(&query_array, top_n, None);
    let raw_scores: Vec<f32> = raw_results.iter().map(|(_, s)| *s).collect();
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
    // `resolve_image_id_for_cosine_path`. Track resolution outcomes
    // for the diagnostic so we can spot path-mapping bugs vs encoder-
    // quality issues.
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
                // Get thumbnail info if available
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

    // Compute query-embedding stats so the diagnostic shows whether
    // the text branch is producing reasonable vectors (mean L2 ~1.0,
    // no NaNs, non-degenerate range).
    let q_norm: f32 = text_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    let q_nan_count = text_embedding.iter().filter(|x| x.is_nan()).count();
    let q_inf_count = text_embedding.iter().filter(|x| x.is_infinite()).count();
    let q_min = text_embedding.iter().cloned().fold(f32::INFINITY, f32::min);
    let q_max = text_embedding.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Diagnostic — full cosine result list for the typed text query.
    // Lets the user audit "blue fish returned X" against the cosine-
    // ranking truth, plus score-distribution stats, query-embedding
    // health, and path-resolution outcomes.
    perf::record_diagnostic(
        "search_query",
        serde_json::json!({
            "type": "semantic",
            "encoder_id": "clip_vit_b_32",
            "top_n": top_n,
            "query_text": query,
            "cosine_cache_size": cache_size,
            "query_embedding": {
                "dim": text_embedding.len(),
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
