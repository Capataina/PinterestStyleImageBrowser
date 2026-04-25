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

/**
 * True if the binary was launched with `--profile`. Cached after the
 * first call: the flag is decided once at process start and never
 * changes, so we don't need to round-trip every render.
 *
 * Returns false if the IPC call fails for any reason — defaulting to
 * "not profiling" is the safe behaviour (no overlay, no breadcrumbs,
 * no recording overhead).
 */
let profilingCache: boolean | null = null;
export async function isProfilingEnabled(): Promise<boolean> {
  if (profilingCache !== null) return profilingCache;
  try {
    profilingCache = await invoke<boolean>("is_profiling_enabled");
  } catch {
    profilingCache = false;
  }
  return profilingCache;
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
