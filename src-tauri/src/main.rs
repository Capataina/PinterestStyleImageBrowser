// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::{db, paths, perf};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() {
    // Profiling is opt-in via a CLI flag OR an env var. Either path
    // works; the env var exists because Tauri 2's CLI has its own
    // `--profile <NAME>` flag (for cargo profile selection), which
    // collides with passing `--profile` through `--` to the binary.
    //
    // Recognised forms:
    //   PROFILING=1 npm run tauri -- dev --release
    //   npm run tauri -- dev --release -- --profiling
    //   cargo tauri dev --release -- --profiling
    //
    // Why this pattern (over a runtime toggle):
    //   - The normal app pays zero perf overhead — `PerfLayer` isn't even
    //     registered, so the `tracing` machinery short-circuits at the
    //     filter and never builds spans for our info-level instruments.
    //   - The frontend mirrors the same flag (via the `is_profiling_enabled`
    //     command), so the overlay shortcut, action breadcrumbs, and any
    //     future profiling UI are dead code in normal runs.
    //   - There's a single, named "profiling mode" — much clearer than a
    //     button somewhere that could be left on by accident.
    let profiling = std::env::args().any(|a| a == "--profiling")
        || std::env::var("PROFILING").map(|v| !v.is_empty() && v != "0").unwrap_or(false);
    perf::set_profiling_enabled(profiling);

    // Subscriber stack:
    // - `fmt::layer` always emits human-readable lines to stderr.
    // - `PerfLayer` ONLY mounts when `--profile` is set. Without it, the
    //   info-level spans this crate emits are still constructed (the env
    //   filter lets them through) but nothing aggregates timings, so the
    //   only cost is one tracing dispatch per call. With it, spans get
    //   recorded into the per-name aggregate + (later) the raw event log.
    //
    // We deliberately don't `.expect()` the init — if a subscriber is
    // already installed (e.g. during cargo test), we'd rather log
    // through the existing one than crash the binary.
    // 6b — both branches were identical (the original intention was a
    // higher verbosity in profiling mode, but the strings drifted to be
    // the same). Collapsed to one filter that we use regardless. The
    // PerfLayer registers separately below; the env filter just controls
    // what the human-readable terminal output shows.
    let _ = profiling; // currently unused; kept above for future use
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,image_browser_lib=info,image_browser=info")
    });

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true));

    if profiling {
        let _ = registry.with(perf::PerfLayer::new()).try_init();
        // Open the session directory under Library/exports/ and start
        // the background flush thread. From this point on, every span
        // close + every record_user_action call lands in
        // Library/exports/perf-{unix_ts}/timeline.jsonl, drained every
        // 5 seconds. The on-exit renderer reads the same file.
        match perf::init_session(paths::exports_dir()) {
            Ok(dir) => {
                tracing::info!(
                    "profiling mode enabled (--profiling or PROFILING=1); session dir: {}",
                    dir.display()
                );
                perf::spawn_flush_thread();
                // Phase 7 — 1Hz RSS/CPU sampler. No-op if profiling
                // is off (the if-let above already gates on that).
                perf::spawn_system_sampler_thread();
            }
            Err(e) => {
                // Don't crash the app if we can't write diagnostics —
                // the user might still want to use it. Aggregates
                // still work via the in-memory PerfLayer.
                tracing::warn!(
                    "could not init profiling session dir: {e}; \
                     timeline.jsonl + exit report will not be written"
                );
            }
        }
    } else {
        let _ = registry.try_init();
    }

    // Pre-Tauri startup work is now minimal: open the SQLite handle and
    // ensure the schema is current. Everything that takes time (model
    // download, scan, thumbnails, embeddings) moved into the indexing
    // pipeline that the Tauri setup() callback spawns on a background
    // thread — see lib.rs::run and indexing.rs.
    //
    // The window opens immediately and the user sees progress over the
    // `indexing-progress` event (Pass 5b renders this as a status pill).
    let db_path = db::ImageDatabase::default_database_path();
    let database = db::ImageDatabase::new(&db_path).expect("failed to init db");
    database.initialize().expect("failed to create tables");

    image_browser_lib::run(database, db_path);
}
