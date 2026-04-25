// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image_browser_lib::{db, perf};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() {
    // Profiling is opt-in via a single CLI flag. With `cargo tauri dev --
    // --profile` (or any args after `--`), the launcher forwards `--profile`
    // straight through to the binary; we pick it up here and stash it in a
    // process-global so the rest of the app can key off one source of truth.
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
    let profiling = std::env::args().any(|a| a == "--profile");
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
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // In profiling mode we want every instrumented span to fire so the
        // report has full coverage. In normal mode we keep the default tame
        // filter — info from us, warn from everything else — so the
        // terminal stays readable.
        if profiling {
            EnvFilter::new("warn,image_browser_lib=info,image_browser=info")
        } else {
            EnvFilter::new("warn,image_browser_lib=info,image_browser=info")
        }
    });

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true));

    if profiling {
        let _ = registry.with(perf::PerfLayer::new()).try_init();
        tracing::info!("profiling mode enabled (--profile)");
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
