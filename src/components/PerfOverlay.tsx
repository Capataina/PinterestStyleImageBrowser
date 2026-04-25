import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Download, RotateCcw, Activity } from "lucide-react";
import {
  exportPerfSnapshot,
  getPerfSnapshot,
  resetPerfStats,
  type PerfSnapshot,
  type SpanSnapshot,
} from "../services/perf";

/**
 * In-app performance diagnostics overlay.
 *
 * Toggle with cmd/ctrl + shift + P. Polls the backend every 2 seconds
 * for an aggregate of every tracing span's timing stats and renders
 * a sortable table. Reset wipes the in-memory accumulator. Export
 * writes the current snapshot to Library/exports/perf-<ts>.json so
 * the user can share it.
 *
 * Position: floating panel docked to the right edge, ~480px wide.
 * Doesn't block interaction with the rest of the app — the user can
 * keep clicking around while watching metrics update live.
 */

interface PerfOverlayProps {
  open: boolean;
  onClose: () => void;
}

const POLL_INTERVAL_MS = 2000;

export function PerfOverlay({ open, onClose }: PerfOverlayProps) {
  const [snapshot, setSnapshot] = useState<PerfSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [exportPath, setExportPath] = useState<string | null>(null);

  // Esc closes
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  // Poll while open
  useEffect(() => {
    if (!open) return;

    let cancelled = false;
    const fetchOnce = async () => {
      try {
        const snap = await getPerfSnapshot();
        if (!cancelled) {
          setSnapshot(snap);
          setErrorMsg(null);
        }
      } catch (e) {
        if (!cancelled) {
          setErrorMsg(e instanceof Error ? e.message : String(e));
        }
      }
    };

    fetchOnce();
    const interval = setInterval(fetchOnce, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [open]);

  const handleReset = async () => {
    setLoading(true);
    try {
      await resetPerfStats();
      const snap = await getPerfSnapshot();
      setSnapshot(snap);
    } finally {
      setLoading(false);
    }
  };

  const handleExport = async () => {
    setLoading(true);
    setExportPath(null);
    try {
      const path = await exportPerfSnapshot();
      setExportPath(path);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop is intentionally subtle — the overlay is
              non-blocking. Click-outside to close. */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.12 }}
            className="fixed inset-0 z-[80] bg-black/30"
            onClick={onClose}
          />
          <motion.aside
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ type: "spring", stiffness: 360, damping: 36 }}
            className="fixed top-0 right-0 z-[81] h-screen w-[480px] max-w-[100vw] flex flex-col bg-card border-l border-border shadow-2xl shadow-black/50"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div className="flex items-center justify-between p-4 border-b border-border bg-card/95 backdrop-blur-md">
              <div className="flex items-center gap-2">
                <Activity className="h-4 w-4 text-primary" />
                <h2 className="text-sm font-semibold">Performance</h2>
                <span className="text-[10px] text-muted-foreground tabular-nums">
                  {snapshot
                    ? `${snapshot.spans.length} spans · refresh ${POLL_INTERVAL_MS / 1000}s`
                    : "loading..."}
                </span>
              </div>
              <button
                onClick={onClose}
                className="rounded p-1 hover:bg-accent transition"
                aria-label="Close performance overlay"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            {/* Toolbar */}
            <div className="flex items-center gap-2 p-3 border-b border-border">
              <button
                onClick={handleReset}
                disabled={loading}
                className="flex items-center gap-1.5 rounded-md bg-secondary px-3 py-1.5 text-xs font-medium hover:bg-accent transition disabled:opacity-50"
              >
                <RotateCcw className="h-3 w-3" />
                Reset
              </button>
              <button
                onClick={handleExport}
                disabled={loading}
                className="flex items-center gap-1.5 rounded-md bg-primary text-primary-foreground px-3 py-1.5 text-xs font-medium hover:opacity-90 transition disabled:opacity-50"
              >
                <Download className="h-3 w-3" />
                Export to Library/exports/
              </button>
              <span className="ml-auto text-[10px] text-muted-foreground">
                ⌘⇧P toggles
              </span>
            </div>

            {/* Export confirmation */}
            {exportPath && (
              <div className="px-3 py-2 bg-primary/10 border-b border-border text-[11px] text-foreground">
                Exported to:{" "}
                <span className="font-mono break-all">{exportPath}</span>
              </div>
            )}

            {/* Error */}
            {errorMsg && (
              <div className="px-3 py-2 bg-destructive/10 border-b border-border text-[11px] text-destructive">
                {errorMsg}
              </div>
            )}

            {/* Table */}
            <div className="flex-1 overflow-y-auto">
              {!snapshot ? (
                <p className="p-4 text-xs text-muted-foreground">
                  Waiting for first snapshot...
                </p>
              ) : snapshot.spans.length === 0 ? (
                <div className="p-4 text-xs text-muted-foreground space-y-2">
                  <p>No spans recorded yet.</p>
                  <p>
                    Trigger an operation (open a folder, click a tile, run a
                    semantic search) and the data will appear here.
                  </p>
                </div>
              ) : (
                <table className="w-full text-[11px] tabular-nums">
                  <thead className="sticky top-0 bg-card border-b border-border">
                    <tr className="text-muted-foreground">
                      <th className="text-left font-medium px-3 py-2">Span</th>
                      <th className="text-right font-medium px-2 py-2">N</th>
                      <th className="text-right font-medium px-2 py-2" title="Mean duration in microseconds">
                        mean
                      </th>
                      <th className="text-right font-medium px-2 py-2" title="50th percentile (median) over recent samples">
                        p50
                      </th>
                      <th className="text-right font-medium px-2 py-2" title="95th percentile over recent samples">
                        p95
                      </th>
                      <th className="text-right font-medium px-3 py-2" title="Maximum duration ever recorded">
                        max
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {snapshot.spans.map((span) => (
                      <SpanRow key={span.name} span={span} />
                    ))}
                  </tbody>
                </table>
              )}
            </div>

            <div className="p-3 border-t border-border text-[10px] text-muted-foreground">
              Sorted by mean duration descending. Spans not yet exercised
              don't appear. Reset to clear, then trigger operations to see
              fresh stats.
            </div>
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}

/** Format microseconds as the most appropriate unit (us, ms, s). */
function formatTime(us: number): string {
  if (us < 1000) return `${us.toFixed(1)}μs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(2)}ms`;
  return `${(us / 1_000_000).toFixed(2)}s`;
}

function SpanRow({ span }: { span: SpanSnapshot }) {
  // Highlight rows whose p95 is unusually high. Heuristic: warn at
  // 100ms, alert at 500ms. The user can drill into the export JSON
  // for full detail.
  const p95Class =
    span.p95_us > 500_000
      ? "text-destructive font-semibold"
      : span.p95_us > 100_000
        ? "text-primary"
        : "text-foreground";

  return (
    <tr className="border-b border-border/50 hover:bg-accent/30 transition-colors">
      <td className="px-3 py-1.5 font-mono break-all">{span.name}</td>
      <td className="text-right px-2 py-1.5 text-muted-foreground">
        {span.count}
      </td>
      <td className="text-right px-2 py-1.5">{formatTime(span.mean_us)}</td>
      <td className="text-right px-2 py-1.5">{formatTime(span.p50_us)}</td>
      <td className={`text-right px-2 py-1.5 ${p95Class}`}>
        {formatTime(span.p95_us)}
      </td>
      <td className="text-right px-3 py-1.5 text-muted-foreground">
        {formatTime(span.max_us)}
      </td>
    </tr>
  );
}
