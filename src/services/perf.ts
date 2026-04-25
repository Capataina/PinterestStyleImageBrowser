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

/**
 * Append a user action to the profiling timeline.
 *
 * No-op when the app isn't in profiling mode — call sites can sprinkle
 * `recordAction(...)` freely without worrying about overhead. The
 * cached `profilingCache` is consulted synchronously after the first
 * `isProfilingEnabled()` resolves; if a call fires before the cache
 * resolves, it just gets dropped (which is fine — early-mount actions
 * are noise).
 *
 * Payload is whatever's useful at the call site: search query, image
 * id, tag id. The on-exit report renderer pairs each user action with
 * span events that fired in the next ~500ms.
 */
export function recordAction(action: string, payload: Record<string, unknown> = {}): void {
  if (!profilingCache) return;
  // Fire-and-forget — don't block the UI thread. If the IPC fails
  // (host crashed mid-session, etc.), we'd rather lose the action
  // than throw an unhandled rejection.
  invoke("record_user_action", { action, payload }).catch(() => {});
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
