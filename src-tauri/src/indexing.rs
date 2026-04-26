//! Background indexing pipeline.
//!
//! Owns the scan → thumbnail → encode flow that previously ran inside
//! main(). Now spawned from a background thread so the Tauri window
//! opens immediately and the user sees progress over the IPC event
//! channel rather than staring at a blank terminal.
//!
//! Two trigger paths:
//!
//! 1. App startup (`run` in lib.rs) — if settings.json has a scan_root
//!    and the model files exist on disk, the setup callback spawns
//!    `run_pipeline` immediately so the catalog refreshes whenever the
//!    user reopens the app.
//! 2. `set_scan_root` IPC command — the user picks a new folder; the
//!    DB is wiped and the same pipeline is spawned to populate it.
//!
//! Concurrency model: a single AtomicBool guards "one indexing run at
//! a time". Trying to start a second run while one is in flight returns
//! `Err(IndexingError::AlreadyRunning)` to the caller. This is a
//! deliberately simple single-flight policy — Pass 5b's UI surfaces
//! the rejection cleanly. A future pass can add cooperative
//! cancellation (cancel-and-restart on rapid root switches) once we
//! see the need.
//!
//! Events: every state change emits a `indexing-progress` Tauri event
//! with an `IndexingProgress` payload. The frontend hook in Pass 5b
//! listens and renders a status pill.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info, warn};

use crate::db::ImageDatabase;
use crate::filesystem::ImageScanner;
use crate::model_download;
use crate::paths;
use crate::similarity_and_semantic_search::cosine_similarity::CosineIndex;
use crate::similarity_and_semantic_search::encoder::ClipImageEncoder;
use crate::similarity_and_semantic_search::encoder_text::ClipTextEncoder;
use crate::thumbnail::ThumbnailGenerator;
use crate::TextEncoderState;

/// The single-flight guard. Wrap in Arc and stash in a Tauri state
/// struct so commands and the setup callback can both reach it.
#[derive(Default)]
pub struct IndexingState {
    pub is_running: AtomicBool,
}

impl IndexingState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Tauri event payload broadcast as the pipeline progresses.
#[derive(Serialize, Clone, Debug)]
pub struct IndexingProgress {
    pub phase: Phase,
    /// How many units of the current phase have been processed.
    pub processed: usize,
    /// Total units in the current phase. Zero means "indeterminate".
    pub total: usize,
    /// Optional human-readable message — paths, error strings, etc.
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Recursively walking the scan root.
    Scan,
    /// Downloading missing model files.
    ModelDownload,
    /// Generating thumbnails for new images.
    Thumbnail,
    /// Producing CLIP image embeddings batch by batch.
    Encode,
    /// Pipeline complete; cosine index repopulated; UI may refresh.
    Ready,
    /// A non-recoverable error stopped the pipeline. `message` carries
    /// a human-readable string. `is_running` has already been cleared
    /// — the user can retry by switching folders or restarting.
    Error,
}

#[derive(Debug)]
pub enum IndexingError {
    AlreadyRunning,
}

impl std::fmt::Display for IndexingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexingError::AlreadyRunning => {
                write!(f, "Indexing is already in progress; wait for it to finish")
            }
        }
    }
}

impl std::error::Error for IndexingError {}

/// Try to start a background indexing run.
///
/// Returns `Ok(())` immediately if a thread was spawned. Returns
/// `Err(AlreadyRunning)` without doing anything if a run is already in
/// flight.
///
/// The spawned thread:
/// 1. Sets `is_running = true` (already done before spawn — see below).
/// 2. Runs scan, model download, thumbnail, encode in order, emitting
///    events between phases and periodically inside long phases.
/// 3. Repopulates the cosine index from the DB so similarity search
///    works without waiting for the next user-triggered query.
/// 4. Emits `Phase::Ready` with the final image count.
/// 5. Sets `is_running = false`.
///
/// On any error inside the pipeline, the thread emits `Phase::Error`
/// with a message and clears `is_running`.
pub fn try_spawn_pipeline(
    app: AppHandle,
    state: Arc<IndexingState>,
    db_path: String,
    cosine_index: Arc<std::sync::Mutex<CosineIndex>>,
    cosine_current_encoder: Arc<std::sync::Mutex<String>>,
) -> Result<(), IndexingError> {
    // Acquire the single-flight slot atomically.
    if state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(IndexingError::AlreadyRunning);
    }

    thread::spawn(move || {
        // RAII guard ensures is_running gets cleared even if the body
        // panics, so a panic doesn't leave the app permanently locked.
        struct RunningGuard(Arc<IndexingState>);
        impl Drop for RunningGuard {
            fn drop(&mut self) {
                self.0.is_running.store(false, Ordering::SeqCst);
            }
        }
        let _guard = RunningGuard(state.clone());

        if let Err(e) = run_pipeline_inner(&app, &db_path, &cosine_index, &cosine_current_encoder) {
            error!("pipeline error: {e}");
            emit(
                &app,
                Phase::Error,
                0,
                0,
                Some(format!("Indexing failed: {e}")),
            );
        }
    });

    Ok(())
}

/// The actual pipeline body. Errors propagate up and become a
/// `Phase::Error` event in the spawning closure.
///
/// `cosine_current_encoder` is the same Arc<Mutex<String>> held by
/// `CosineIndexState.current_encoder_id` — the pipeline writes into
/// it after the priority encoder's phase finishes, so the in-memory
/// cache and the "what's loaded" marker stay in sync without the next
/// search command needing to repopulate.
#[tracing::instrument(name = "pipeline.run", skip(app, cosine_index, cosine_current_encoder))]
fn run_pipeline_inner(
    app: &AppHandle,
    db_path: &str,
    cosine_index: &Arc<std::sync::Mutex<CosineIndex>>,
    cosine_current_encoder: &Arc<std::sync::Mutex<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 0. Try the on-disk cosine cache before doing anything else. On a
    //    typical second-launch with no new images, the cache is fresh
    //    and similarity / semantic search become available essentially
    //    immediately — the user can act on the catalog while the rest
    //    of the pipeline (rescan, model verification) finishes in the
    //    background. The cache is invalidated automatically if the DB
    //    file is newer than it.
    {
        let db_path_buf = std::path::PathBuf::from(db_path);
        if let Ok(mut idx) = cosine_index.lock() {
            if idx.cached_images.is_empty() {
                idx.load_from_disk_if_fresh(&db_path_buf);
            }
        }
    }

    // 1. Make sure the model files are on disk. The download function
    //    now reports progress via callback so the UI's status pill can
    //    render a real determinate bar across the ~1 GB of downloads
    //    instead of the previous "Checking models..." flash followed
    //    by a multi-minute silent stretch.
    emit(app, Phase::ModelDownload, 0, 0, Some("Checking models...".into()));
    let app_for_progress = app.clone();
    let progress_cb = move |processed: u64, total: u64, current_file: Option<&str>| {
        let msg = current_file.map(|f| {
            if total > 0 {
                format!(
                    "Downloading {f} — {} / {} MB",
                    processed / 1_048_576,
                    total / 1_048_576
                )
            } else {
                format!("Downloading {f} — {} MB", processed / 1_048_576)
            }
        });
        // Tauri events take usize; cast carefully. The download is
        // capped at ~1.2 GB so usize::MAX is not in play even on 32-bit.
        let processed = processed.min(usize::MAX as u64) as usize;
        let total = total.min(usize::MAX as u64) as usize;
        emit(&app_for_progress, Phase::ModelDownload, processed, total, msg);
    };
    if let Err(e) = model_download::download_models_if_missing(progress_cb) {
        // Non-fatal: scan + thumbnail still work without models. Encode
        // gets skipped further down.
        warn!("model download skipped: {e}");
        emit(
            app,
            Phase::ModelDownload,
            0,
            0,
            Some(format!("Model download skipped: {e}")),
        );
    }

    // 1b. Pre-warm the text encoder so the user's first semantic search
    //     doesn't pause for ~1-2 seconds while the ONNX session and
    //     tokenizer load. We're already on a background thread inside
    //     the pipeline; absorbing the load cost here is invisible to
    //     the user (the indexing pill is already showing), whereas
    //     paying it later means the user types a query and stares at
    //     a spinner.
    {
        let models_dir = paths::models_dir();
        let model_path = models_dir.join(crate::model_download::CLIP_TEXT_FILENAME);
        let tokenizer_path = models_dir.join(crate::model_download::CLIP_TOKENIZER_FILENAME);
        if model_path.exists() && tokenizer_path.exists() {
            // Bind the State separately so its lifetime extends across
            // the full block. Inlining `app.state::<TextEncoderState>()
            // .encoder.lock()` produces a temporary that the borrow
            // checker drops too early.
            let text_encoder_state: tauri::State<'_, TextEncoderState> =
                app.state::<TextEncoderState>();
            let lock_result = text_encoder_state.encoder.lock();
            if let Ok(mut lock) = lock_result {
                if lock.is_none() {
                    info!("pre-warming text encoder");
                    match ClipTextEncoder::new(&model_path, &tokenizer_path) {
                        Ok(encoder) => *lock = Some(encoder),
                        Err(e) => warn!("text encoder pre-warm failed: {e}"),
                    }
                }
            }
        }
    }

    // 2. Open a fresh DB handle. Mutex<Connection> coexists with the
    //    Tauri-managed one (rusqlite supports multiple connections to
    //    the same file).
    let database = ImageDatabase::new(db_path)?;
    database.initialize()?;

    // 3. Resolve the list of roots to scan. Multi-folder support means
    //    we walk every enabled root and tag each image with its source.
    let all_roots = database.list_roots()?;
    let enabled_roots: Vec<_> = all_roots.iter().filter(|r| r.enabled).collect();
    if enabled_roots.is_empty() {
        // Nothing to do — empty-state UI covers this case.
        emit(
            app,
            Phase::Ready,
            0,
            0,
            Some("No folders configured".into()),
        );
        return Ok(());
    }

    // 4. Scan every enabled root. We aggregate paths across roots so
    //    progress reflects total work, not per-folder progress.
    let _scan_phase = tracing::info_span!("pipeline.scan_phase").entered();
    emit(
        app,
        Phase::Scan,
        0,
        0,
        Some(format!("Scanning {} folder(s)", enabled_roots.len())),
    );
    let scanner = ImageScanner::new();

    // First pass: walk every enabled root, collect (path, root_id)
    // tuples. We keep per-root path sets so we can run the orphan
    // detection pass per-root in a moment.
    let mut all_paths: Vec<(String, i64)> = Vec::new();
    let mut paths_per_root: std::collections::HashMap<i64, Vec<String>> =
        std::collections::HashMap::new();
    for root in &enabled_roots {
        let root_path = std::path::Path::new(&root.path);
        if !root_path.exists() {
            warn!(
                "configured root {} no longer exists; skipping",
                root.path
            );
            continue;
        }
        match scanner.scan_directory(root_path) {
            Ok(paths) => {
                let entry = paths_per_root.entry(root.id).or_default();
                for p in paths {
                    entry.push(p.clone());
                    all_paths.push((p, root.id));
                }
            }
            Err(e) => {
                warn!("scan of {} failed: {e}", root.path);
            }
        }
    }
    let total_found = all_paths.len();

    // Second pass: insert into DB. Idempotent — INSERT OR IGNORE on the
    // path uniqueness constraint means existing rows aren't duplicated.
    for (i, (path, root_id)) in all_paths.iter().enumerate() {
        database.add_image(path.clone(), Some(*root_id))?;
        if (i + 1) % 100 == 0 || i + 1 == total_found {
            emit(app, Phase::Scan, i + 1, total_found, None);
        }
    }

    // Orphan-detection pass: for each enabled root, mark any DB row
    // whose path isn't in the just-scanned alive set as orphaned. The
    // grid query filters orphaned rows out, so the user doesn't see
    // tiles for files that were deleted between launches.
    for root in &enabled_roots {
        let alive = paths_per_root.remove(&root.id).unwrap_or_default();
        match database.mark_orphaned(root.id, &alive) {
            Ok(n) if n > 0 => {
                info!("orphan-detection: {} rows marked orphaned in root {}", n, root.path);
            }
            Ok(_) => {}
            Err(e) => warn!("orphan-detection for root {} failed: {e}", root.path),
        }
    }

    emit(app, Phase::Scan, total_found, total_found, None);
    drop(_scan_phase);

    // 5. Thumbnails + encoder run in PARALLEL.
    //
    // Previously these were sequential phases — first thumbnails for
    // every image (rayon-parallel), then embeddings (single-threaded
    // CLIP) — so on a fresh ~1500-image library the user stared at
    // a partially-empty grid for ~80s waiting for the encoder to
    // finish. Now they run concurrently:
    //
    //   - The thumbnail rayon loop occupies the CPU pool
    //   - The encoder runs on a dedicated thread with its own DB
    //     connection (WAL mode lets concurrent writes coexist —
    //     thumbnails write `thumbnail_path/width/height`, the encoder
    //     writes `embedding`; different columns, no row-level conflict)
    //
    // The thumbnail phase finishes first (rayon makes it ~10× faster
    // than encoder per-image), so the user sees a fully populated
    // grid in ~7s while the encoder cooks in the background. Cosine
    // repopulate joins both before running.
    //
    // The encoder phase span lives inside its spawned thread so the
    // perf report still attributes its time correctly.
    let image_model_path = paths::models_dir().join(crate::model_download::CLIP_VISION_FILENAME);
    let encoder_handle: Option<thread::JoinHandle<Result<(), String>>> =
        if image_model_path.exists() {
            let app = app.clone();
            let db_path_for_encoder = db_path.to_string();
            let model_path = image_model_path.clone();
            // Clone the cosine state Arcs into the spawned thread so
            // it can hot-populate the cache after the priority encoder
            // finishes — without waiting for the bulk populate at the
            // end of run_pipeline_inner.
            let cosine_index_for_encoder = cosine_index.clone();
            let cosine_current_encoder_for_encoder = cosine_current_encoder.clone();
            Some(thread::spawn(move || {
                run_encoder_phase(
                    &app,
                    &db_path_for_encoder,
                    &model_path,
                    &cosine_index_for_encoder,
                    &cosine_current_encoder_for_encoder,
                )
            }))
        } else {
            warn!(
                "{} missing; embeddings will be \
                 empty until next launch.",
                crate::model_download::CLIP_VISION_FILENAME
            );
            None
        };

    let _thumb_phase = tracing::info_span!("pipeline.thumbnail_phase").entered();
    //
    //    Per-image cost is dominated by JPEG decode+encode, which is
    //    embarrassingly parallel. The DB write under the mutex is
    //    microseconds vs ~100ms decode/encode, so contention there is
    //    negligible. On an M-series chip with 8-12 cores this gives
    //    a ~6-10x speedup vs the previous serial loop.
    let thumbnail_generator = ThumbnailGenerator::new(&paths::thumbnails_dir(), 400, 400)?;
    let needs_thumbs = database.get_images_without_thumbnails()?;
    let total_thumbs = needs_thumbs.len();
    if total_thumbs > 0 {
        emit(app, Phase::Thumbnail, 0, total_thumbs, None);

        let completed = AtomicUsize::new(0);
        let last_emit_bucket = AtomicUsize::new(0);
        // Coalesce progress emits to roughly every 25 thumbnails
        // (across ALL workers combined) so a 1500-image run fires
        // ~60 events rather than ~1500.
        const EMIT_EVERY: usize = 25;

        // Build a map from path -> root_id so each thumbnail lands in
        // the right per-root subfolder. Single SELECT — was N+1 before
        // (audit finding): `get_root_id_by_path` per image-needing-
        // thumbnail held the DB Mutex 1500 times in rapid succession on
        // a typical first run. The new `get_paths_to_root_ids` returns
        // the entire (path, root_id) map in one query, matching the
        // pattern `populate_from_db` already uses for embeddings.
        //
        // unwrap_or_default preserves the previous failure semantic:
        // if the SELECT fails, downstream `generate_thumbnail` falls
        // back to the legacy flat thumbnail directory (root_id None).
        let path_to_root = database.get_paths_to_root_ids().unwrap_or_default();

        needs_thumbs.par_iter().for_each(|image| {
            let root_id = path_to_root.get(&image.path).copied().flatten();
            match thumbnail_generator.generate_thumbnail(
                Path::new(&image.path),
                image.id,
                root_id,
            ) {
                Ok(result) => {
                    if let Err(e) = database.update_image_thumbnail(
                        image.id,
                        &result.thumbnail_path,
                        result.original_width,
                        result.original_height,
                    ) {
                        warn!("DB update for thumbnail of image {} failed: {e}", image.id);
                    }
                }
                Err(e) => {
                    warn!("thumbnail generation failed for {}: {e}", image.path);
                }
            }

            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let bucket = done / EMIT_EVERY;
            let prev = last_emit_bucket.load(Ordering::Relaxed);
            if bucket > prev
                && last_emit_bucket
                    .compare_exchange(prev, bucket, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                emit(app, Phase::Thumbnail, done, total_thumbs, None);
            }
        });
    }
    emit(app, Phase::Thumbnail, total_thumbs, total_thumbs, None);
    drop(_thumb_phase);

    // Join the encoder thread that was spawned before the thumbnail
    // loop. Cosine repopulate (next step) needs every embedding
    // committed to disk first, so this barrier is load-bearing.
    if let Some(handle) = encoder_handle {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!("encoder phase failed: {e}"),
            Err(_) => warn!("encoder thread panicked"),
        }
    }

    // 7. Final safety-net cosine populate.
    //
    //    The per-encoder hot-populate inside run_encoder_phase already
    //    loaded the priority encoder's cache as soon as that encoder's
    //    phase finished — so by the time we get here, the cache is
    //    almost always already correct. This block is a safety net for
    //    two cases:
    //      (a) priority encoder didn't get a chance to run (e.g. its
    //          model file was missing — we fall back to CLIP);
    //      (b) the cache held stale state from a previous session and
    //          the priority encoder happened to be one we don't run.
    //
    //    Either way we resolve the priority + populate for it. This is
    //    the canonical "what's loaded matches the user's pick" point
    //    and the only place that triggers `save_to_disk`, so the next
    //    launch starts hot.
    let priority = crate::settings::Settings::load()
        .priority_image_encoder
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "clip_vit_b_32".to_string());
    let _cosine_phase = tracing::info_span!("pipeline.cosine_repopulate", encoder = %priority).entered();
    // Lock order: current_encoder_id first, then index — must match
    // CosineIndexState::ensure_loaded_for to keep the search path
    // deadlock-free.
    if let (Ok(mut cur), Ok(mut idx)) =
        (cosine_current_encoder.lock(), cosine_index.lock())
    {
        // Skip the DB read if the per-encoder hot-populate inside
        // run_encoder_phase already loaded this same encoder — a
        // common case for the priority encoder.
        let already_loaded = *cur == priority && !idx.cached_images.is_empty();
        if !already_loaded {
            idx.populate_from_db_for_encoder(&database, &priority);
            *cur = priority.clone();
        }
        idx.save_to_disk();
    }
    drop(_cosine_phase);

    // 8. Done — total image count is what the user sees in the grid.
    let final_count = database.get_all_images().map(|v| v.len()).unwrap_or(0);
    emit(
        app,
        Phase::Ready,
        final_count,
        final_count,
        Some(format!("{final_count} images indexed")),
    );

    Ok(())
}

/// Encoder phase — runs the CLIP image encoder over every row that
/// doesn't yet have an embedding, in batches of 32. Lives on its own
/// thread (spawned from `run_pipeline_inner` in parallel with the
/// thumbnail rayon loop) so its single-threaded ONNX inference doesn't
/// block the multi-core JPEG codec work that happens alongside.
///
/// Opens its own `ImageDatabase` (separate SQLite connection) so it
/// doesn't contend with the thumbnail thread for the same Mutex.
/// WAL mode (initialise() pragmas) makes the concurrent writes safe —
/// thumbnails write the `thumbnail_path/width/height` columns, this
/// writes `embedding`; no row-level conflict because the columns are
/// disjoint and SQLite's WAL serialises commits at the page level
/// without blocking readers.
fn run_encoder_phase(
    app: &AppHandle,
    db_path: &str,
    image_model_path: &Path,
    cosine_index: &Arc<std::sync::Mutex<CosineIndex>>,
    cosine_current_encoder: &Arc<std::sync::Mutex<String>>,
) -> Result<(), String> {
    // Encodes through three image encoders sequentially. Order is
    // dynamic — the user's `priority_image_encoder` setting (if any)
    // runs FIRST so its embeddings land in the DB ASAP and the cosine
    // cache hot-populates for it as soon as the phase finishes. The
    // other two then run in their default order behind it. This
    // addresses the "I picked DINOv2 but encoding spent 20 minutes on
    // SigLIP-2 first" UX problem.
    //
    // Default order (no priority set, or priority unrecognised):
    //   1. CLIP-ViT-B/32 (also written to legacy images.embedding for
    //      back-compat with semantic_search's existing call site)
    //   2. SigLIP-2-Base
    //   3. DINOv2-Base
    //
    // Each encoder's results land in the embeddings table keyed by
    // (image_id, encoder_id), so swapping encoders in Settings later
    // doesn't require re-encoding.
    //
    // Sequential rather than parallel because each encoder is single-
    // threaded ONNX inference — running them all in parallel would
    // contend on the same CPU cores. Future optimisation: use rayon's
    // pool to interleave preprocessing across encoders.
    let _encode_phase = tracing::info_span!("pipeline.encode_phase").entered();

    let database = ImageDatabase::new(db_path).map_err(|e| e.to_string())?;

    // Read the priority pick once at phase start; ignore None / empty /
    // unknown so the default order applies as a safe fallback.
    let priority = crate::settings::Settings::load()
        .priority_image_encoder
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    // Default order, then move priority to the front (if recognised).
    let mut order: Vec<&str> = vec!["clip_vit_b_32", "siglip2_base", "dinov2_base"];
    if let Some(pos) = order.iter().position(|e| *e == priority.as_str()) {
        let p = order.remove(pos);
        order.insert(0, p);
    }
    info!("encoder order this run: {order:?} (priority={priority:?})");

    let siglip2_path = paths::models_dir().join(
        crate::similarity_and_semantic_search::encoder_siglip2::SIGLIP2_IMAGE_MODEL_FILENAME,
    );
    let dinov2_path = paths::models_dir().join(
        crate::similarity_and_semantic_search::encoder_dinov2::DINOV2_IMAGE_MODEL_FILENAME,
    );

    for encoder_id in order {
        match encoder_id {
            "clip_vit_b_32" => {
                run_clip_encoder(app, &database, image_model_path)?;
            }
            "siglip2_base" => {
                if siglip2_path.exists() {
                    run_trait_encoder(
                        app,
                        &database,
                        "siglip2_base",
                        || crate::similarity_and_semantic_search::encoder_siglip2::Siglip2ImageEncoder::new(&siglip2_path),
                    )?;
                } else {
                    warn!("SigLIP-2 image model missing at {}; skipping", siglip2_path.display());
                    continue; // don't hot-populate for an encoder that didn't run
                }
            }
            "dinov2_base" => {
                if dinov2_path.exists() {
                    run_trait_encoder(
                        app,
                        &database,
                        crate::similarity_and_semantic_search::encoder_dinov2::DINOV2_ENCODER_ID,
                        || crate::similarity_and_semantic_search::encoder_dinov2::Dinov2ImageEncoder::new(&dinov2_path),
                    )?;
                } else {
                    warn!("DINOv2 image model missing at {}; skipping", dinov2_path.display());
                    continue;
                }
            }
            _ => continue,
        }

        // Per-encoder hot-populate of the cosine cache. Only fires for
        // the priority encoder — the cache holds ONE encoder at a time,
        // and the user's image-image picker drives which one. As soon
        // as that encoder's phase finishes, image-image becomes
        // searchable without waiting for the other encoders.
        //
        // Lock order MUST match `CosineIndexState::ensure_loaded_for`
        // (current_encoder_id first, then index). Reversing the order
        // here would risk a deadlock against a concurrent search call,
        // and a write-only-to-index-then-id sequence opens a window
        // where a search reads the old id but the freshly-populated
        // cache and silently wipes it by repopulating for the old id.
        if encoder_id == priority {
            info!("hot-populating cosine cache for priority encoder '{encoder_id}'");
            if let (Ok(mut cur), Ok(mut idx)) =
                (cosine_current_encoder.lock(), cosine_index.lock())
            {
                idx.populate_from_db_for_encoder(&database, encoder_id);
                *cur = encoder_id.to_string();
            }
        }
    }

    Ok(())
}

/// CLIP encoder phase — writes BOTH the legacy `images.embedding`
/// column (kept for backward-compat with semantic_search's existing
/// reader) AND the new `embeddings` table row keyed by encoder_id.
/// This double-write goes away in a future migration once everyone
/// has re-indexed.
fn run_clip_encoder(
    app: &AppHandle,
    database: &ImageDatabase,
    model_path: &Path,
) -> Result<(), String> {
    let needs_embed = database
        .get_images_without_embeddings()
        .map_err(|e| e.to_string())?;
    let total = needs_embed.len();
    if total == 0 {
        return Ok(());
    }
    emit(app, Phase::Encode, 0, total, Some("Encoding (CLIP)".into()));

    let run_started = std::time::Instant::now();
    let mut encoder = ClipImageEncoder::new(model_path).map_err(|e| e.to_string())?;
    const BATCH_SIZE: usize = 32;
    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed_paths: Vec<String> = Vec::new();
    let mut sample_emitted = false;
    for chunk in needs_embed.chunks(BATCH_SIZE) {
        let batch_paths: Vec<&Path> = chunk.iter().map(|i| Path::new(&i.path)).collect();
        match encoder.encode_batch(&batch_paths) {
            Ok(embeddings) => {
                // Preprocessing/embedding sample diagnostic — fires
                // once per encoder run on the first successful batch.
                // Captures dim, L2 norm, range, NaN count of the first
                // embedding so the report can show "CLIP first encoded
                // image: 512-d, norm=1.000, range [-0.18, 0.21], no
                // NaNs" — pinpoints encoder breakage early.
                if !sample_emitted {
                    if let (Some(first_path), Some(first_emb)) =
                        (chunk.first(), embeddings.first())
                    {
                        emit_preprocessing_sample("clip_vit_b_32", &first_path.path, first_emb);
                        sample_emitted = true;
                    }
                }
                // R1 — single-transaction batch write of every row in
                // this chunk. R8 — legacy_clip_too = false: we no
                // longer double-write to the legacy images.embedding
                // column. The schema bump to pipeline version 3 wipes
                // any stale legacy data, so the cosine populate
                // fallback path (cosine/index.rs) just sees an empty
                // legacy column on first re-index and reads from the
                // per-encoder embeddings table only.
                let batch_rows: Vec<(crate::db::ID, Vec<f32>)> = chunk
                    .iter()
                    .zip(embeddings.iter())
                    .map(|(image, emb)| (image.id, emb.clone()))
                    .collect();
                let row_count = batch_rows.len();
                match database.upsert_embeddings_batch(
                    "clip_vit_b_32",
                    &batch_rows,
                    false,
                ) {
                    Ok(()) => succeeded += row_count,
                    Err(e) => {
                        // Whole batch failed at the DB level — record
                        // every row as a write failure rather than
                        // pretending some succeeded.
                        let err_str = e.to_string();
                        for image in chunk.iter() {
                            failed_paths.push(format!("{}: db batch — {err_str}", image.path));
                        }
                    }
                }
            }
            Err(e) => {
                // Whole batch failed — record each path as a failure
                // with the shared error rather than aborting indexing.
                let err_str = e.to_string();
                for image in chunk.iter() {
                    failed_paths.push(format!("{}: encode_batch — {}", image.path, err_str));
                }
            }
        }
        processed += chunk.len();
        emit(app, Phase::Encode, processed, total, Some("Encoding (CLIP)".into()));
        // R3 — drain the WAL between batches so it can't grow without
        // bound under wal_autocheckpoint=0. PASSIVE never blocks
        // foreground readers; it just folds whatever pages are clean
        // back into the main DB.
        let _ = database.checkpoint_passive();
    }

    // Emit a per-encoder run summary so the report shows
    // "CLIP attempted 1842, succeeded 1840, failed 2 (sample paths
    // ...), mean 12.3 ms/image". Failed-path samples turn an opaque
    // 0.1% failure rate into something a user can diagnose.
    let elapsed_ms = run_started.elapsed().as_millis() as u64;
    let mean_per_image_ms = if processed > 0 { elapsed_ms as f64 / processed as f64 } else { 0.0 };
    crate::perf::record_diagnostic(
        "encoder_run_summary",
        serde_json::json!({
            "encoder_id": "clip_vit_b_32",
            "attempted": processed,
            "succeeded": succeeded,
            "failed": failed_paths.len(),
            "elapsed_ms": elapsed_ms,
            "mean_per_image_ms": mean_per_image_ms,
            "failed_sample": failed_paths.iter().take(10).cloned().collect::<Vec<_>>(),
        }),
    );
    Ok(())
}

/// Generic per-encoder loop using the ImageEncoder trait. Used for
/// SigLIP-2 + DINOv2; each writes only to the new embeddings table.
fn run_trait_encoder<F, E>(
    app: &AppHandle,
    database: &ImageDatabase,
    encoder_id: &str,
    make_encoder: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<E, Box<dyn std::error::Error>>,
    E: crate::similarity_and_semantic_search::encoders::ImageEncoder,
{
    use std::path::Path as StdPath;
    let needs = database
        .get_images_without_embedding_for(encoder_id)
        .map_err(|e| e.to_string())?;
    let total = needs.len();
    if total == 0 {
        return Ok(());
    }
    let label = format!("Encoding ({encoder_id})");
    emit(app, Phase::Encode, 0, total, Some(label.clone()));

    let run_started = std::time::Instant::now();
    let mut encoder = make_encoder().map_err(|e| e.to_string())?;
    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed_paths: Vec<String> = Vec::new();
    let mut sample_emitted = false;
    // Trait default `encode_batch` falls back to one-by-one. Future:
    // override per encoder if batching is faster.
    for chunk in needs.chunks(32) {
        let paths: Vec<&StdPath> = chunk.iter().map(|(_, p)| StdPath::new(p)).collect();
        match encoder.encode_batch(&paths) {
            Ok(embeddings) => {
                if !sample_emitted {
                    if let (Some((_, first_path)), Some(first_emb)) =
                        (chunk.first(), embeddings.first())
                    {
                        emit_preprocessing_sample(encoder_id, first_path, first_emb);
                        sample_emitted = true;
                    }
                }
                // R1 — same BEGIN IMMEDIATE batch write as the CLIP
                // path. legacy_clip_too = false because only CLIP
                // double-writes to the legacy column.
                let batch_rows: Vec<(crate::db::ID, Vec<f32>)> = chunk
                    .iter()
                    .zip(embeddings.iter())
                    .map(|((id, _), emb)| (*id, emb.clone()))
                    .collect();
                let row_count = batch_rows.len();
                match database.upsert_embeddings_batch(
                    encoder_id,
                    &batch_rows,
                    false,
                ) {
                    Ok(()) => succeeded += row_count,
                    Err(e) => {
                        let err_str = e.to_string();
                        for (_, path) in chunk.iter() {
                            failed_paths
                                .push(format!("{path}: db batch — {err_str}"));
                        }
                    }
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                for (_, path) in chunk.iter() {
                    failed_paths.push(format!("{}: encode_batch — {}", path, err_str));
                }
            }
        }
        processed += chunk.len();
        emit(app, Phase::Encode, processed, total, Some(label.clone()));
        // R3 — drain WAL between batches under wal_autocheckpoint=0.
        let _ = database.checkpoint_passive();
    }

    // Per-encoder run summary diagnostic — same shape as the CLIP
    // path. Lets the report show side-by-side cost + failure rates
    // across CLIP / SigLIP-2 / DINOv2.
    let elapsed_ms = run_started.elapsed().as_millis() as u64;
    let mean_per_image_ms = if processed > 0 { elapsed_ms as f64 / processed as f64 } else { 0.0 };
    crate::perf::record_diagnostic(
        "encoder_run_summary",
        serde_json::json!({
            "encoder_id": encoder_id,
            "attempted": processed,
            "succeeded": succeeded,
            "failed": failed_paths.len(),
            "elapsed_ms": elapsed_ms,
            "mean_per_image_ms": mean_per_image_ms,
            "failed_sample": failed_paths.iter().take(10).cloned().collect::<Vec<_>>(),
        }),
    );
    Ok(())
}

/// Emit a `preprocessing_sample` diagnostic for the first image
/// encoded by an encoder. Captures embedding-side stats (dim, L2
/// norm, value range, NaN/Inf counts) — these reflect both the
/// preprocessing pipeline AND the encoder's output health in one
/// shot. Cheap (microseconds).
fn emit_preprocessing_sample(encoder_id: &str, image_path: &str, embedding: &[f32]) {
    let dim = embedding.len();
    let nan_count = embedding.iter().filter(|x| x.is_nan()).count();
    let inf_count = embedding.iter().filter(|x| x.is_infinite()).count();
    let finite: Vec<f32> = embedding.iter().filter(|x| x.is_finite()).copied().collect();
    let (min, max, mean, l2_norm) = if finite.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        let min = finite.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = finite.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mean = finite.iter().sum::<f32>() / finite.len() as f32;
        let l2 = finite.iter().map(|x| x * x).sum::<f32>().sqrt();
        (min, max, mean, l2)
    };
    let interpretation = if nan_count > 0 || inf_count > 0 {
        "BROKEN — NaN/Inf in embedding (preprocessing or encoder bug)"
    } else if l2_norm < 0.01 {
        "WARNING — near-zero norm; encoder produced degenerate output"
    } else if (l2_norm - 1.0).abs() < 0.01 {
        "OK — L2-normalised unit vector"
    } else {
        "Non-normalised — cosine still works since math divides by norms"
    };
    crate::perf::record_diagnostic(
        "preprocessing_sample",
        serde_json::json!({
            "encoder_id": encoder_id,
            "first_image_path": image_path,
            "embedding_dim": dim,
            "l2_norm": l2_norm,
            "min": min,
            "max": max,
            "mean": mean,
            "nan_count": nan_count,
            "inf_count": inf_count,
            "first_8_dims": embedding.iter().take(8).copied().collect::<Vec<f32>>(),
            "interpretation": interpretation,
        }),
    );
}

fn emit(
    app: &AppHandle,
    phase: Phase,
    processed: usize,
    total: usize,
    message: Option<String>,
) {
    let payload = IndexingProgress {
        phase,
        processed,
        total,
        message,
    };
    if let Err(e) = app.emit("indexing-progress", &payload) {
        // Don't crash on emit failure — just log. Receivers may have
        // disconnected (closing window mid-pipeline).
        warn!("failed to emit event: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_state_default_not_running() {
        let s = IndexingState::new();
        assert!(!s.is_running.load(Ordering::SeqCst));
    }

    #[test]
    fn phase_serialises_kebab_case() {
        let progress = IndexingProgress {
            phase: Phase::ModelDownload,
            processed: 1,
            total: 3,
            message: None,
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(
            json.contains("\"phase\":\"model-download\""),
            "expected kebab-case phase, got {json}"
        );
    }

    #[test]
    fn ready_phase_serialises() {
        let progress = IndexingProgress {
            phase: Phase::Ready,
            processed: 42,
            total: 42,
            message: Some("done".into()),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"phase\":\"ready\""));
        assert!(json.contains("\"processed\":42"));
        assert!(json.contains("\"message\":\"done\""));
    }

    #[test]
    fn single_flight_first_acquire_succeeds() {
        // Direct test of the AtomicBool gate semantics that
        // try_spawn_pipeline relies on. We don't actually spawn the
        // pipeline (it'd need a Tauri app handle) — just exercise
        // the compare_exchange behaviour.
        let state = IndexingState::new();
        let acquired = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(acquired.is_ok());
        // Now the slot is held; second attempt should fail.
        let denied = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(denied.is_err());
    }

    #[test]
    fn single_flight_releases_after_clear() {
        let state = IndexingState::new();
        state.is_running.store(true, Ordering::SeqCst);
        // Simulate the RAII guard's drop behaviour.
        state.is_running.store(false, Ordering::SeqCst);
        // The slot is open again.
        let reacquired = state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(reacquired.is_ok());
    }

    #[test]
    fn indexing_error_displays_human_readable_message() {
        let err = IndexingError::AlreadyRunning;
        let msg = format!("{err}");
        assert!(
            msg.contains("already in progress"),
            "expected human-readable AlreadyRunning message, got {msg}"
        );
    }

    #[test]
    fn all_phases_serialise_to_kebab_case() {
        for (variant, expected_str) in [
            (Phase::Scan, "scan"),
            (Phase::ModelDownload, "model-download"),
            (Phase::Thumbnail, "thumbnail"),
            (Phase::Encode, "encode"),
            (Phase::Ready, "ready"),
            (Phase::Error, "error"),
        ] {
            let progress = IndexingProgress {
                phase: variant,
                processed: 0,
                total: 0,
                message: None,
            };
            let json = serde_json::to_string(&progress).unwrap();
            let needle = format!("\"phase\":\"{}\"", expected_str);
            assert!(
                json.contains(&needle),
                "Phase {expected_str:?} did not serialise as expected: {json}"
            );
        }
    }
}
