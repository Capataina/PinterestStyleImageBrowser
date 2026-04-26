use std::sync::atomic::{AtomicBool, Ordering};
use tauri::State;
use tracing::{debug, info, warn};

use crate::commands::{resolve_image_id_for_cosine_path, ApiError, ImageSearchResult};
use crate::db::ImageDatabase;
use crate::perf;
use crate::similarity_and_semantic_search::cosine::rrf::{
    reciprocal_rank_fusion, RankedList, DEFAULT_K_RRF,
};
use crate::{CosineIndexState, FusionIndexState};

/// Has the once-per-session cross-encoder comparison fired yet?
/// Cross-encoder comparison is expensive (builds a temporary
/// CosineIndex per other encoder) — we only want one snapshot per
/// session to compare encoder rankings side-by-side. Subsequent
/// View-Similar calls skip the comparison cost.
static CROSS_ENCODER_RAN: AtomicBool = AtomicBool::new(false);

/// Run the cross-encoder comparison diagnostic for an image-image
/// query. For each *other* available encoder, builds a temporary
/// CosineIndex from that encoder's embeddings, runs top-5 against
/// the query image's embedding in that encoder's space, and emits
/// a single diagnostic with all encoders' top-5 results side-by-side.
///
/// Lets the user answer "would DINOv2 have ranked these images
/// differently than CLIP did?" without manually switching encoders
/// and re-running the search.
fn run_cross_encoder_comparison(
    db: &ImageDatabase,
    image_id: i64,
    active_encoder: &str,
) {
    use crate::similarity_and_semantic_search::cosine::CosineIndex;
    use ndarray::Array1;
    use std::path::PathBuf;

    let started = std::time::Instant::now();
    // Active encoders only — `dinov2_small` is the legacy 384-d ID
    // that was migrated away in pipeline-version 2 (rows wiped). Including
    // it here would log a noise `cosine_cache_populated: count=0` per
    // "View Similar" click and waste a populate roundtrip.
    let all_encoders = ["clip_vit_b_32", "dinov2_base", "siglip2_base"];
    let exclude_path: Option<PathBuf> = db
        .get_all_images()
        .ok()
        .and_then(|imgs| imgs.into_iter().find(|i| i.id == image_id).map(|i| PathBuf::from(i.path)));

    let mut per_encoder: Vec<serde_json::Value> = Vec::new();
    for enc in all_encoders {
        if enc == active_encoder {
            // Active encoder's results are already in the main
            // search_query diagnostic — no need to duplicate.
            continue;
        }
        let enc_started = std::time::Instant::now();
        // Pull this encoder's embedding for the query image. Falls
        // back gracefully — empty embeddings table for an encoder
        // means we just record "no embeddings".
        let q_emb = if enc == "clip_vit_b_32" {
            db.get_image_embedding(image_id).ok()
        } else {
            db.get_embedding(image_id, enc).ok()
        };
        let q_emb = match q_emb.filter(|v| !v.is_empty()) {
            Some(v) => v,
            None => {
                per_encoder.push(serde_json::json!({
                    "encoder_id": enc,
                    "status": "no_embedding_for_query_image",
                }));
                continue;
            }
        };

        let mut tmp = CosineIndex::new();
        tmp.populate_from_db_for_encoder(db, enc);
        let cache_size = tmp.cached_images.len();
        if cache_size == 0 {
            per_encoder.push(serde_json::json!({
                "encoder_id": enc,
                "status": "no_cache_embeddings",
            }));
            continue;
        }
        let q = Array1::from_vec(q_emb);
        let results = tmp.get_similar_images_sorted(&q, 5, exclude_path.as_ref());
        per_encoder.push(serde_json::json!({
            "encoder_id": enc,
            "status": "ok",
            "cache_size": cache_size,
            "top5": results.iter().map(|(p, s)| serde_json::json!({
                "path": p.to_string_lossy(),
                "score": *s,
            })).collect::<Vec<_>>(),
            "elapsed_ms": enc_started.elapsed().as_millis() as u64,
        }));
    }

    perf::record_diagnostic(
        "cross_encoder_comparison",
        serde_json::json!({
            "fired_for_image_id": image_id,
            "active_encoder": active_encoder,
            "comparison_results": per_encoder,
            "total_elapsed_ms": started.elapsed().as_millis() as u64,
            "note": "Fires once per session — first View-Similar after launch. Subsequent searches skip the cross-encoder cost.",
        }),
    );
}

/// Phase 5 — multi-encoder rank fusion for image-image similarity.
///
/// Replaces the tiered "1 of top 5, 5 of top 25" sampling strategy
/// with Reciprocal Rank Fusion across every available encoder. The
/// fused output naturally surfaces images that *all three* encoders
/// agree are similar (CLIP for concept overlap + DINOv2 for visual
/// structure + SigLIP-2 for descriptive content), which is both more
/// accurate AND more diverse than any single-encoder ranking.
///
/// The user no longer pays the "we randomly skipped some good results
/// to get diversity" tax — diversity emerges from inter-encoder
/// disagreement on what counts as similar.
///
/// Implementation:
/// 1. For each encoder family (CLIP, SigLIP-2, DINOv2): pull the
///    query image's per-encoder embedding from the DB. Skip encoders
///    that don't have an embedding for this image yet (graceful
///    fallback — fusion still works with whichever encoders are
///    indexed).
/// 2. Score the query against that encoder's per-image embeddings
///    via FusionIndexState.ranked_for_encoder, getting top-K.
/// 3. Apply RRF over the 1-3 ranked lists to produce one fused list.
/// 4. Resolve paths → image ids + thumbnails like the other similarity
///    commands.
///
/// `top_n`: how many fused results to return.
/// `per_encoder_top_k`: how many top results from each encoder to
///   feed into the fusion. Defaults to `5 * top_n` (~150 for top_n=30)
///   so the fusion has enough candidate diversity from each encoder.
#[tauri::command]
#[tracing::instrument(
    name = "ipc.get_fused_similar_images",
    skip(db, fusion_state),
    fields(image_id, top_n, per_encoder_top_k)
)]
pub fn get_fused_similar_images(
    db: State<'_, ImageDatabase>,
    fusion_state: State<'_, FusionIndexState>,
    image_id: i64,
    top_n: usize,
    per_encoder_top_k: Option<usize>,
) -> Result<Vec<ImageSearchResult>, ApiError> {
    use ndarray::Array1;
    use std::path::PathBuf;

    let per_encoder_top_k = per_encoder_top_k.unwrap_or(top_n.saturating_mul(5).max(50));
    info!(
        "get_fused_similar_images - image_id: {image_id}, top_n: {top_n}, \
         per_encoder_top_k: {per_encoder_top_k}"
    );

    let started = std::time::Instant::now();
    let all_images = db.get_all_images()?;
    let exclude_path = all_images
        .iter()
        .find(|img| img.id == image_id)
        .map(|img| PathBuf::from(&img.path));

    // Encoder set is the same as cross_encoder_comparison —
    // dinov2_small is the legacy 384-d id, excluded.
    const FUSION_ENCODERS: &[&str] = &["clip_vit_b_32", "siglip2_base", "dinov2_base"];

    let mut ranked_lists: Vec<RankedList> = Vec::with_capacity(FUSION_ENCODERS.len());
    let mut per_encoder_diag: Vec<serde_json::Value> = Vec::new();

    for enc in FUSION_ENCODERS {
        let enc_started = std::time::Instant::now();
        // Pull this encoder's embedding for the query image.
        let q_emb = db.get_embedding(image_id, enc).ok();
        let q_emb = match q_emb.filter(|v| !v.is_empty()) {
            Some(v) => v,
            None => {
                per_encoder_diag.push(serde_json::json!({
                    "encoder_id": enc,
                    "status": "no_embedding_for_query_image",
                    "elapsed_ms": enc_started.elapsed().as_millis() as u64,
                }));
                continue;
            }
        };
        let q = Array1::from_vec(q_emb);
        let ranked = fusion_state
            .ranked_for_encoder(
                &db,
                enc,
                &q,
                per_encoder_top_k,
                exclude_path.as_ref(),
            )
            .map_err(ApiError::Cosine)?;
        let count = ranked.len();
        if count == 0 {
            per_encoder_diag.push(serde_json::json!({
                "encoder_id": enc,
                "status": "empty_ranked_list_for_encoder",
                "elapsed_ms": enc_started.elapsed().as_millis() as u64,
            }));
            continue;
        }
        ranked_lists.push(RankedList {
            encoder_id: (*enc).to_string(),
            items: ranked.clone(),
        });
        per_encoder_diag.push(serde_json::json!({
            "encoder_id": enc,
            "status": "ok",
            "ranked_count": count,
            "top5_paths": ranked.iter().take(5)
                .map(|(p, s)| serde_json::json!({"path": p.to_string_lossy(), "score": *s}))
                .collect::<Vec<_>>(),
            "elapsed_ms": enc_started.elapsed().as_millis() as u64,
        }));
    }

    if ranked_lists.is_empty() {
        info!("Fusion: no encoder produced a ranked list — returning empty");
        return Ok(Vec::new());
    }

    let fused = reciprocal_rank_fusion(&ranked_lists, DEFAULT_K_RRF, top_n);

    // Resolve paths → ImageSearchResult, with the same path-resolution
    // + thumbnail-enrichment shape the other similarity commands use.
    let mut resolution_misses: Vec<String> = Vec::new();
    let mut thumb_misses: u32 = 0;
    let results: Vec<ImageSearchResult> = fused
        .iter()
        .filter_map(|f| {
            match resolve_image_id_for_cosine_path(&db, &f.path, Some(&all_images)) {
                Some((id, final_path)) => {
                    let thumb_info = db.get_image_thumbnail_info(id).ok().flatten();
                    if thumb_info.is_none() {
                        thumb_misses += 1;
                    }
                    let (thumbnail_path, width, height) = thumb_info
                        .map(|(tp, w, h)| (Some(tp), Some(w), Some(h)))
                        .unwrap_or((None, None, None));
                    Some(ImageSearchResult {
                        id,
                        path: final_path,
                        // The "score" surfaced to the frontend is the
                        // fused RRF score. It's bounded roughly between
                        // 0 and N_encoders × 1/(k+1) (≈ 0.05 for 3
                        // encoders + k=60), not the [0,1] cosine range
                        // the single-encoder paths return — frontends
                        // that present this score should label it
                        // "Fused" rather than "Cosine similarity".
                        score: f.fused_score,
                        thumbnail_path,
                        width,
                        height,
                    })
                }
                None => {
                    resolution_misses.push(f.path.to_string_lossy().into_owned());
                    None
                }
            }
        })
        .collect();

    perf::record_diagnostic(
        "search_query",
        serde_json::json!({
            "type": "fused",
            "top_n": top_n,
            "per_encoder_top_k": per_encoder_top_k,
            "k_rrf": DEFAULT_K_RRF,
            "query_image_id": image_id,
            "query_image_path": exclude_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            "encoders_used": ranked_lists
                .iter()
                .map(|r| r.encoder_id.clone())
                .collect::<Vec<_>>(),
            "encoders_skipped": FUSION_ENCODERS.len() - ranked_lists.len(),
            "fused_result_count": fused.len(),
            "resolved_count": results.len(),
            "thumbnail_misses": thumb_misses,
            "missed_paths_sample":
                resolution_misses.iter().take(10).cloned().collect::<Vec<_>>(),
            "per_encoder": per_encoder_diag,
            "fused_top10_with_evidence": fused.iter().take(10).map(|f| serde_json::json!({
                "path": f.path.to_string_lossy(),
                "fused_score": f.fused_score,
                "per_encoder_evidence": f.per_encoder.iter().map(|(e, r, s)| serde_json::json!({
                    "encoder_id": e,
                    "rank": r,
                    "encoder_score": s,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "total_elapsed_ms": started.elapsed().as_millis() as u64,
        }),
    );

    info!(
        "get_fused_similar_images returning {} results (used {} encoders, {} ms)",
        results.len(),
        ranked_lists.len(),
        started.elapsed().as_millis(),
    );

    Ok(results)
}

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
    let cache_size = index.cached_images.len();
    let raw_results = index.get_tiered_similar_images(&query, exclude_path.as_ref());
    let raw_scores: Vec<f32> = raw_results.iter().map(|(_, s)| *s).collect();

    // Path resolution + thumbnail enrichment. The dimensions used to
    // be fetched frontend-side via N parallel `getImageSize` DOM image
    // loads (audit Performance finding) — moved to backend here so
    // the result lands fully-populated in one IPC round-trip. Uses
    // the same `db.get_image_thumbnail_info` helper that
    // `semantic_search` already calls.
    //
    // We track per-path resolution outcomes so the diagnostic below
    // can show "raw cosine returned 35 results, 33 resolved to image
    // ids, 2 missed (paths: ...)" — pinpoints whether bad search is
    // due to encoder quality or path-mapping bugs.
    let mut resolution_misses: Vec<String> = Vec::new();
    let mut thumb_misses: u32 = 0;
    let results: Vec<ImageSearchResult> = raw_results
        .iter()
        .cloned()
        .filter_map(|(path, score)| {
            match resolve_image_id_for_cosine_path(&db, &path, Some(&all_images)) {
                Some((id, final_path)) => {
                    let thumb_info = db.get_image_thumbnail_info(id).ok().flatten();
                    if thumb_info.is_none() {
                        thumb_misses += 1;
                    }
                    let (thumbnail_path, width, height) = thumb_info
                        .map(|(tp, w, h)| (Some(tp), Some(w), Some(h)))
                        .unwrap_or((None, None, None));
                    Some(ImageSearchResult {
                        id,
                        path: final_path,
                        score,
                        thumbnail_path,
                        width,
                        height,
                    })
                }
                None => {
                    resolution_misses.push(path.to_string_lossy().into_owned());
                    None
                }
            }
        })
        .collect();

    // Diagnostic: dump the FULL cosine result list (paths + scores)
    // plus score-distribution stats and path-resolution outcomes.
    // Lets the user audit whether bad search results are an
    // encoder-quality issue (cosine returned the wrong things), a
    // path-mapping bug (right things returned but couldn't be mapped
    // to image ids), or a thumbnail-enrichment issue.
    perf::record_diagnostic(
        "search_query",
        serde_json::json!({
            "type": "tiered_similar",
            "encoder_id": encoder_id,
            "query_image_id": image_id,
            "query_image_path": exclude_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "cosine_cache_size": cache_size,
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
        "get_tiered_similar_images returning {} results",
        results.len()
    );

    // Fire the cross-encoder comparison diagnostic once per session.
    // compare_exchange ensures only the first arriving View-Similar
    // call pays the cost (~50-200 ms × number of other encoders).
    if perf::is_profiling_enabled()
        && CROSS_ENCODER_RAN
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        // Drop the cosine_state lock before running comparison —
        // the comparison function builds its own temporary indexes.
        drop(index);
        run_cross_encoder_comparison(&db, image_id, &encoder_id);
    }

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
    let cache_size = index.cached_images.len();
    let raw_results = index.get_similar_images(&query, top_n, exclude_path.as_ref());
    let raw_scores: Vec<f32> = raw_results.iter().map(|(_, s)| *s).collect();
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
    // 60-line lookup block). Resolution outcomes tracked for the
    // diagnostic so we can spot path-mapping bugs vs encoder-quality
    // issues.
    let mut resolution_misses: Vec<String> = Vec::new();
    let mut thumb_misses: u32 = 0;
    let results: Vec<ImageSearchResult> = raw_results
        .iter()
        .cloned()
        .filter_map(|(path, score)| {
            let info = resolve_image_id_for_cosine_path(&db, &path, Some(&all_images));
            if info.is_none() {
                warn!(
                    "  Failed to map path to id - path: {:?}",
                    path.file_name().unwrap_or_default()
                );
                resolution_misses.push(path.to_string_lossy().into_owned());
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
                let thumb_info = db.get_image_thumbnail_info(id).ok().flatten();
                if thumb_info.is_none() {
                    thumb_misses += 1;
                }
                let (thumbnail_path, width, height) = thumb_info
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

    // Diagnostic — same shape as the tiered version's diagnostic.
    perf::record_diagnostic(
        "search_query",
        serde_json::json!({
            "type": "similar",
            "encoder_id": encoder_id,
            "top_n": top_n,
            "query_image_id": image_id,
            "query_image_path": exclude_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "cosine_cache_size": cache_size,
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

    // Cross-encoder comparison — once per session (see top of file).
    if perf::is_profiling_enabled()
        && CROSS_ENCODER_RAN
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        drop(index);
        run_cross_encoder_comparison(&db, image_id, &encoder_id);
    }

    Ok(results)
}
