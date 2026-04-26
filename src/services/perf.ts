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

/**
 * Timed IPC wrapper. Drop-in replacement for `invoke<T>(cmd, args)`:
 * times the round-trip with performance.now() and records it as a
 * user-action breadcrumb (kind="ipc_call") so the on-exit report can
 * correlate frontend-observed IPC latency with the backend span that
 * served the call.
 *
 * Why this exists separately from backend instrumentation: the
 * backend's `#[tracing::instrument]` only measures the time spent
 * inside the Rust handler. The frontend's perceived latency includes
 * IPC serialisation, the Tauri runtime hop, response deserialisation,
 * and any await scheduling — the gap between this and the backend
 * span tells you whether the IPC layer itself is a bottleneck.
 *
 * No-op overhead when profiling is off: just calls invoke directly.
 *
 * Returns the same Promise<T> as raw invoke; errors propagate
 * unchanged (and are still recorded with `ok: false`).
 */
export async function perfInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!profilingCache) {
    return invoke<T>(cmd, args);
  }
  const start = performance.now();
  try {
    const result = await invoke<T>(cmd, args);
    const duration_ms = performance.now() - start;
    recordAction("ipc_call", { command: cmd, duration_ms, ok: true });
    return result;
  } catch (e) {
    const duration_ms = performance.now() - start;
    recordAction("ipc_call", {
      command: cmd,
      duration_ms,
      ok: false,
      error: String(e),
    });
    throw e;
  }
}

/**
 * Callback for React.Profiler — records every render of a wrapped
 * component subtree as an action breadcrumb. Wire it into JSX as:
 *
 *   <Profiler id="MyComponent" onRender={onRenderProfiler}>
 *     <MyComponent ... />
 *   </Profiler>
 *
 * No-op when profiling is off (recordAction short-circuits).
 *
 * The 5ms threshold filters out trivial renders that would otherwise
 * dominate the timeline — the report cares about renders that might
 * cause perceptible jank, not the every-keystroke noise.
 */
export function onRenderProfiler(
  id: string,
  phase: "mount" | "update" | "nested-update",
  actualDuration: number,
): void {
  if (actualDuration < 5) return;
  recordAction("render", {
    component: id,
    phase,
    duration_ms: Math.round(actualDuration * 100) / 100,
  });
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
