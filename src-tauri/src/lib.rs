use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

use crate::{
    db::ImageDatabase,
    indexing::IndexingState,
    similarity_and_semantic_search::cosine_similarity::CosineIndex,
    similarity_and_semantic_search::encoder_text::ClipTextEncoder,
};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod commands;
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

pub struct CosineIndexState {
    /// Wrapped in Arc<Mutex<...>> rather than plain Mutex<...> so the
    /// indexing thread (Pass 5) can hold a clone alongside the Tauri-
    /// managed state. Both point at the same in-memory cache.
    ///
    /// The cache holds embeddings from ONE encoder at a time —
    /// whichever the user's `imageEncoder` setting selects. When the
    /// setting changes, the cache is wiped + repopulated from the
    /// embeddings table for the new encoder. Repopulate is fast
    /// because the embeddings are already on disk; only the new
    /// encoder's embeddings need DB → memory transfer.
    pub index: Arc<Mutex<CosineIndex>>,
    pub db_path: String,
    /// The encoder_id whose embeddings are currently loaded into
    /// `index`. Empty string when uninitialised. Read at search-time
    /// to detect stale cache after a settings change.
    pub current_encoder_id: Arc<Mutex<String>>,
}

impl CosineIndexState {
    /// Ensure `index` holds the cache for `encoder_id`. If it already
    /// does, returns immediately (no work). Otherwise repopulates
    /// from `db.get_all_embeddings_for(encoder_id)` — fast because
    /// the on-disk embeddings are already there from the indexing
    /// pass; this is just DB→memory transfer.
    ///
    /// Returns an error if the DB read fails. Callers should treat
    /// this as a hard failure for the search command — there's no
    /// useful "partial cache" state to fall back to.
    /// Drop the in-memory cache AND the "currently loaded encoder"
    /// marker so the very next search call repopulates from the DB.
    ///
    /// Why both: `ensure_loaded_for` short-circuits when
    /// `current_encoder_id` matches the requested encoder, and only
    /// repopulates the cache otherwise. If we cleared the cache but
    /// left the marker, the next search would short-circuit and run
    /// against an empty cache (image-image returns 0; semantic search
    /// only saves itself via a separate empty-cache fallback in
    /// `commands/semantic.rs`). Worse, the previous code's leftover
    /// pre-toggle entries kept appearing in results because nothing
    /// forced a reload — exactly the "disabled folder still shows in
    /// View Similar" bug. Lock order matches `ensure_loaded_for`
    /// (current_encoder_id then index) to keep the search path
    /// deadlock-free.
    pub fn invalidate(&self) {
        if let (Ok(mut cur), Ok(mut idx)) =
            (self.current_encoder_id.lock(), self.index.lock())
        {
            idx.cached_images.clear();
            cur.clear();
        }
    }

    pub fn ensure_loaded_for(
        &self,
        db: &ImageDatabase,
        encoder_id: &str,
    ) -> Result<(), String> {
        // Two-step lock acquisition: check current id first to short-
        // circuit the common case (encoder hasn't changed). Acquire
        // the index Mutex only if a reload is actually needed.
        {
            let cur = self
                .current_encoder_id
                .lock()
                .map_err(|e| format!("current_encoder_id mutex poisoned: {e}"))?;
            if *cur == encoder_id {
                return Ok(());
            }
        }
        // Need to switch — take both locks. The order is consistent
        // (id first, then index) across all callers; deadlock-safe.
        let mut cur = self
            .current_encoder_id
            .lock()
            .map_err(|e| format!("current_encoder_id mutex poisoned: {e}"))?;
        let mut index = self
            .index
            .lock()
            .map_err(|e| format!("index mutex poisoned: {e}"))?;
        // Re-check under the lock (another thread might've switched).
        if *cur == encoder_id {
            return Ok(());
        }
        index.populate_from_db_for_encoder(db, encoder_id);
        *cur = encoder_id.to_string();
        Ok(())
    }
}

/// State for the text encoders used in semantic search.
///
/// Each encoder is lazy-loaded on first use. We hold one slot per
/// supported family (CLIP — 512-d English BPE; SigLIP-2 — 768-d
/// Gemma SentencePiece) so the user can switch the text encoder in
/// the picker mid-session without paying the model-load cost again
/// when they swap back.
///
/// Two slots not three because DINOv2 is image-only — there is no
/// DINOv2 text branch to dispatch through.
pub struct TextEncoderState {
    /// CLIP English text encoder. 512-d output. Default.
    pub encoder: Mutex<Option<ClipTextEncoder>>,
    /// SigLIP-2 base 256 text encoder. 768-d output, Gemma SentencePiece
    /// tokenizer (256k vocab). The picker dispatches semantic_search
    /// here when the user has SigLIP-2 selected as the text encoder.
    pub siglip2_encoder: Mutex<
        Option<crate::similarity_and_semantic_search::encoder_siglip2::Siglip2TextEncoder>,
    >,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(db: ImageDatabase, db_path: String) {
    use commands::encoders::{list_available_encoders, set_priority_image_encoder};
    use commands::images::{get_images, get_pipeline_stats};
    use commands::notes::{get_image_notes, set_image_notes};
    use commands::profiling::{
        export_perf_snapshot, get_perf_snapshot, is_profiling_enabled, record_user_action,
        reset_perf_stats,
    };
    use commands::roots::{
        add_root, get_scan_root, list_roots, remove_root, set_root_enabled, set_scan_root,
    };
    use commands::semantic::semantic_search;
    use commands::similarity::{get_similar_images, get_tiered_similar_images};
    use commands::tags::{
        add_tag_to_image, create_tag, delete_tag, get_tags, remove_tag_from_image,
    };

    let cosine_index = Arc::new(Mutex::new(CosineIndex::new()));
    let current_encoder_id = Arc::new(Mutex::new(String::new()));
    let cosine_state = CosineIndexState {
        index: cosine_index.clone(),
        db_path: db_path.clone(),
        current_encoder_id: current_encoder_id.clone(),
    };

    // Text encoder state (lazy-loaded on first semantic search).
    // Phase 4 — both encoders coexist so the picker can switch
    // dispatch instantly without paying the model-load cost twice.
    let text_encoder_state = TextEncoderState {
        encoder: Mutex::new(None),
        siglip2_encoder: Mutex::new(None),
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
                // Startup diagnostic: snapshot of what's on disk +
                // what's already encoded. Lets the on-exit report's
                // Diagnostics section show "this session started with
                // X CLIP embeddings, Y SigLIP-2, Z DINOv2" — very
                // useful for the "I selected DINOv2 but got 0 results"
                // bug class.
                if perf::is_profiling_enabled() {
                    if let Ok(temp_db) = ImageDatabase::new(&db_path) {
                        let _ = temp_db.initialize();
                        let stats = temp_db.get_pipeline_stats().ok();
                        let models_dir = paths::models_dir();
                        let model_files: Vec<String> = std::fs::read_dir(&models_dir)
                            .ok()
                            .map(|entries| {
                                entries
                                    .filter_map(|e| e.ok())
                                    .map(|e| e.file_name().to_string_lossy().into_owned())
                                    .collect()
                            })
                            .unwrap_or_default();
                        perf::record_diagnostic(
                            "startup_state",
                            serde_json::json!({
                                "db_path": &db_path,
                                "models_dir": models_dir.display().to_string(),
                                "model_files_present": model_files,
                                "embedding_counts_per_encoder": stats.as_ref().map(|s| {
                                    s.with_embedding_per_encoder.iter().map(|e| {
                                        serde_json::json!({
                                            "encoder_id": e.encoder_id,
                                            "count": e.count,
                                        })
                                    }).collect::<Vec<_>>()
                                }),
                                "total_images": stats.as_ref().map(|s| s.total_images),
                                "with_thumbnail": stats.as_ref().map(|s| s.with_thumbnail),
                                "orphaned": stats.as_ref().map(|s| s.orphaned),
                            }),
                        );
                    }

                    // Cosine math sanity check — synthetic vectors with
                    // known expected outputs. If this ever returns a
                    // surprising number, EVERY semantic-search /
                    // similarity result downstream is suspect because
                    // the math itself is broken. Cheap (~µs).
                    {
                        use ndarray::Array1;
                        use crate::similarity_and_semantic_search::cosine::CosineIndex;
                        let a = Array1::from_vec(vec![1.0_f32, 0.0, 0.0]);
                        let b = Array1::from_vec(vec![0.0_f32, 1.0, 0.0]);
                        let c = Array1::from_vec(vec![1.0_f32, 0.0, 0.0]);
                        let d = Array1::from_vec(vec![-1.0_f32, 0.0, 0.0]);
                        let zero = Array1::from_vec(vec![0.0_f32, 0.0, 0.0]);
                        let high_dim_a: Array1<f32> = Array1::from_vec((0..512).map(|i| (i as f32).sin()).collect());
                        let high_dim_b: Array1<f32> = Array1::from_vec((0..512).map(|i| (i as f32).cos()).collect());

                        let orthogonal = CosineIndex::cosine_similarity(&a, &b);
                        let parallel = CosineIndex::cosine_similarity(&a, &c);
                        let opposite = CosineIndex::cosine_similarity(&a, &d);
                        let zero_vec = CosineIndex::cosine_similarity(&a, &zero);
                        let dim_mismatch = CosineIndex::cosine_similarity(&a, &high_dim_a);
                        let high_dim_random = CosineIndex::cosine_similarity(&high_dim_a, &high_dim_b);

                        perf::record_diagnostic(
                            "cosine_math_sanity",
                            serde_json::json!({
                                "orthogonal_3d":   { "got": orthogonal,    "expected": 0.0,  "passes": orthogonal.abs() < 1e-5 },
                                "parallel_3d":     { "got": parallel,      "expected": 1.0,  "passes": (parallel - 1.0).abs() < 1e-5 },
                                "opposite_3d":     { "got": opposite,      "expected": -1.0, "passes": (opposite + 1.0).abs() < 1e-5 },
                                "zero_vector_3d":  { "got": zero_vec,      "expected": 0.0,  "passes": zero_vec.abs() < 1e-5 },
                                "dim_mismatch":    { "got": dim_mismatch,  "expected": 0.0,  "passes": dim_mismatch.abs() < 1e-5, "note": "3-d vs 512-d should return 0 via guard, not panic" },
                                "high_dim_random": { "got": high_dim_random, "expected_range": "[-0.1, 0.1] for sin/cos quasi-orthogonal", "passes": high_dim_random.abs() < 0.2 },
                                "interpretation": "All passes=true means cosine math is correct — bad search results are an encoder/data issue, not math.",
                            }),
                        );
                    }
                }

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
                    current_encoder_id.clone(),
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
                        current_encoder_id.clone(),
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
            get_pipeline_stats,
            list_available_encoders,
            set_priority_image_encoder,
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
