/**
 * Performance diagnostics IPC wrappers.
 *
 * Backed by tracing-subscriber Layer (src-tauri/src/perf.rs) that
 * accumulates per-span-name stats. The overlay polls
 * getPerfSnapshot() and renders the result.
 */
import { invoke } from "@tauri-apps/api/core";

export interface SpanSnapshot {
  name: string;
  count: number;
  mean_us: number;
  min_us: number;
  max_us: number;
  p50_us: number;
  p95_us: number;
  p99_us: number;
  recent_window: number;
}

export interface PerfSnapshot {
  spans: SpanSnapshot[];
  /** Unix epoch seconds when the snapshot was taken */
  timestamp: number;
}

export async function getPerfSnapshot(): Promise<PerfSnapshot> {
  return await invoke<PerfSnapshot>("get_perf_snapshot");
}

export async function resetPerfStats(): Promise<void> {
  await invoke("reset_perf_stats");
}

/**
 * Write the current snapshot to Library/exports/perf-<unix-ts>.json.
 * Returns the absolute path of the written file (so the UI can show
 * a confirmation with the location).
 */
export async function exportPerfSnapshot(): Promise<string> {
  return await invoke<string>("export_perf_snapshot");
}
