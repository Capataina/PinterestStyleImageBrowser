use crate::paths;
use crate::perf;

/// True if the binary was launched with `--profile`. The frontend
/// reads this once at startup to decide whether to mount the perf
/// overlay, register the cmd+shift+P shortcut, and emit user-action
/// breadcrumbs. Without the flag, all of those paths stay dormant
/// and the app pays no profiling cost.
#[tauri::command]
pub fn is_profiling_enabled() -> bool {
    perf::is_profiling_enabled()
}

/// Returns aggregated span timing stats for the in-app perf overlay.
/// Frontend polls this to render the live diagnostics panel.
#[tauri::command]
pub fn get_perf_snapshot() -> perf::PerfSnapshot {
    perf::snapshot()
}

/// Wipe collected perf stats. Useful between scenarios when measuring
/// a specific operation in isolation.
#[tauri::command]
pub fn reset_perf_stats() -> Result<(), String> {
    perf::reset();
    Ok(())
}

/// Append a user action to the profiling timeline. No-op when the
/// app isn't in profiling mode (the frontend checks this before
/// calling, but we double-check on the backend so a stale
/// profilingCache can't poison the timeline).
///
/// Payload is free-form JSON — call sites attach whatever's relevant
/// (query text, image id, tag id, sort mode...). The on-exit
/// markdown renderer correlates these with span events that fired
/// in the next ~500ms.
#[tauri::command]
pub fn record_user_action(action: String, payload: serde_json::Value) {
    perf::record_user_action(action, payload);
}

/// Write the current perf snapshot to Library/exports/perf-<unix-ts>.json
/// as pretty-printed JSON. Returns the absolute path so the frontend
/// can show it in a confirmation message.
#[tauri::command]
pub fn export_perf_snapshot() -> Result<String, String> {
    let snap = perf::snapshot();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = paths::exports_dir().join(format!("perf-{now}.json"));
    let json = serde_json::to_string_pretty(&snap)
        .map_err(|e| format!("Failed to serialise snapshot: {e}"))?;
    std::fs::write(&dest, json)
        .map_err(|e| format!("Failed to write export: {e}"))?;
    Ok(dest.to_string_lossy().into_owned())
}
