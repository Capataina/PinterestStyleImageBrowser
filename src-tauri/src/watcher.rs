//! Filesystem watcher for live integrity.
//!
//! Watches every enabled root recursively. Filesystem events are
//! debounced (5s default) and trigger an incremental rescan via the
//! indexing pipeline: new image files appear in the DB, files that
//! have disappeared get marked `orphaned = 1` (the orphan column is
//! set by the rescan-aware filesystem scan, lib.rs Phase 7).
//!
//! Lifecycle:
//! - `start` runs once during the Tauri setup callback.
//! - The returned debouncer holds the actual watch threads. Dropping
//!   it cancels everything (it's stored in a Tauri-managed state).
//! - The watcher does NOT auto-reconfigure when roots are added or
//!   removed. After add_root or remove_root we'd want to restart the
//!   watcher; for now, the indexing pipeline that those commands
//!   trigger handles the immediate rescan and the watcher keeps
//!   watching whatever it had at startup. A future iteration can
//!   rebuild the watcher on root changes.
//!
//! Implementation notes:
//! - notify-debouncer-mini is used because raw notify events can
//!   fire dozens of times per "save" on macOS (every metadata change,
//!   every fsync). 5s of debouncing collapses a typical bulk add
//!   (dropping 100 photos into a folder) into one rescan.
//! - The rescan re-spawns the indexing pipeline. The single-flight
//!   guard in indexing::try_spawn_pipeline means rapid changes during
//!   an in-progress rescan are coalesced — the second event tries to
//!   spawn, gets AlreadyRunning back, and is silently dropped. The
//!   user sees a single rescan covering everything that changed.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::AppHandle;
use tracing::{debug, info, warn};

use crate::indexing;
use crate::similarity_and_semantic_search::cosine_similarity::CosineIndex;

/// Type alias to keep the Debouncer's full type out of public API
/// signatures (it's gnarly with the watcher backend's generics).
pub type WatcherHandle = Debouncer<notify::RecommendedWatcher>;

/// Spin up watchers for every enabled root path. The returned handle
/// must outlive the app — typically held in a Tauri-managed state
/// struct. Returns None if no roots are enabled or if the underlying
/// notify backend can't be initialised on the current platform.
#[tracing::instrument(name = "watcher.start", skip(app, indexing_state, cosine_index))]
pub fn start(
    app: AppHandle,
    paths_to_watch: Vec<PathBuf>,
    db_path: String,
    indexing_state: Arc<indexing::IndexingState>,
    cosine_index: Arc<std::sync::Mutex<CosineIndex>>,
) -> Option<WatcherHandle> {
    if paths_to_watch.is_empty() {
        info!("watcher: no enabled roots, skipping watcher init");
        return None;
    }

    let app_for_handler = app.clone();
    let db_path_for_handler = db_path.clone();
    let indexing_state_for_handler = indexing_state.clone();
    let cosine_index_for_handler = cosine_index.clone();

    let mut debouncer = match new_debouncer(
        Duration::from_secs(5),
        move |result: DebounceEventResult| {
            // Wrap the event handler in a manual span so each debounce
            // batch gets timed individually. Closure-based callbacks
            // can't carry #[tracing::instrument] directly.
            let _span = tracing::info_span!("watcher.event").entered();
            match result {
                Ok(events) => {
                    debug!(
                        "watcher: {} events received, triggering rescan",
                        events.len()
                    );
                    let _ = indexing::try_spawn_pipeline(
                        app_for_handler.clone(),
                        indexing_state_for_handler.clone(),
                        db_path_for_handler.clone(),
                        cosine_index_for_handler.clone(),
                    );
                }
                Err(e) => {
                    warn!("watcher debounce error: {e:?}");
                }
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            warn!("could not initialise filesystem watcher: {e}");
            return None;
        }
    };

    let watcher = debouncer.watcher();
    for path in &paths_to_watch {
        match watcher.watch(path, RecursiveMode::Recursive) {
            Ok(()) => info!("watching {} (recursive)", path.display()),
            Err(e) => warn!("could not watch {}: {e}", path.display()),
        }
    }

    Some(debouncer)
}
