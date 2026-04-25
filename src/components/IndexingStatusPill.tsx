import { motion, AnimatePresence } from "framer-motion";
import { Loader2, AlertCircle, CheckCircle2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  useIndexingProgress,
  type IndexingPhase,
} from "../hooks/useIndexingProgress";

const PHASE_LABELS: Record<IndexingPhase, string> = {
  scan: "Scanning",
  "model-download": "Downloading models",
  thumbnail: "Generating thumbnails",
  encode: "Encoding embeddings",
  ready: "Ready",
  error: "Error",
};

/**
 * Floating status pill in the top-right corner.
 *
 * Visibility:
 * - Hidden until the first event arrives (no flash on cold launch).
 * - Shown for any active phase.
 * - "ready" lingers 4s as a "X images indexed" confirmation, fades.
 * - "error" sticks until the user dismisses (×) or a new run starts.
 *
 * Visual: minimal — icon + label + counter + thin progress bar. We
 * deliberately dropped the redundant "message" detail line that the
 * previous design had; the phase label tells you what the counter
 * counts. The full message is available on hover via `title=`.
 */
export function IndexingStatusPill() {
  const { progress } = useIndexingProgress();
  const [dismissed, setDismissed] = useState(false);
  const [showFinal, setShowFinal] = useState(false);

  useEffect(() => {
    if (!progress) return;
    if (progress.phase === "ready") {
      setShowFinal(true);
      setDismissed(false);
      const t = setTimeout(() => setShowFinal(false), 4000);
      return () => clearTimeout(t);
    }
    if (progress.phase === "error") {
      setShowFinal(false);
      setDismissed(false);
      return;
    }
    setDismissed(false);
    setShowFinal(false);
  }, [progress?.phase]);

  if (!progress) return null;
  if (dismissed) return null;
  if (progress.phase === "ready" && !showFinal) return null;

  const isError = progress.phase === "error";
  const isReady = progress.phase === "ready";
  const label = PHASE_LABELS[progress.phase];

  const fill =
    progress.total > 0 ? Math.min(1, progress.processed / progress.total) : 0;
  const showBar = progress.total > 0 && !isError && !isReady;

  return (
    <AnimatePresence>
      <motion.div
        key="indexing-pill"
        initial={{ opacity: 0, y: -8, scale: 0.95 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: -8, scale: 0.95 }}
        transition={{ type: "spring", stiffness: 400, damping: 30 }}
        className={[
          "fixed top-4 right-4 z-50 flex items-center gap-3",
          "rounded-full border bg-card/95 backdrop-blur-md",
          "px-4 py-2 shadow-lg shadow-black/30",
          "min-w-[240px] max-w-[380px]",
          isError ? "border-destructive/40" : "border-border",
        ].join(" ")}
        title={progress.message ?? undefined}
      >
        <div className="shrink-0">
          {isError ? (
            <AlertCircle className="h-4 w-4 text-destructive" />
          ) : isReady ? (
            <CheckCircle2 className="h-4 w-4 text-primary" />
          ) : (
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
          )}
        </div>

        <div className="flex flex-1 flex-col gap-1.5 overflow-hidden">
          <div className="flex items-center justify-between gap-2">
            <span
              className={[
                "text-xs font-medium",
                isError ? "text-destructive" : "text-foreground",
              ].join(" ")}
            >
              {label}
            </span>
            {progress.total > 0 && !isError && !isReady && (
              <span className="text-[10px] tabular-nums text-muted-foreground">
                {humanize(progress.processed, progress.total, progress.phase)}
              </span>
            )}
          </div>

          {showBar && (
            <div className="h-1 w-full overflow-hidden rounded-full bg-muted">
              <motion.div
                className="h-full bg-primary"
                initial={false}
                animate={{ width: `${fill * 100}%` }}
                transition={{ type: "spring", stiffness: 200, damping: 30 }}
              />
            </div>
          )}

          {isReady && progress.message && (
            <span className="text-[10px] text-muted-foreground">
              {progress.message}
            </span>
          )}
        </div>

        {(isError || isReady) && (
          <button
            type="button"
            onClick={() => setDismissed(true)}
            aria-label="Dismiss"
            className="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground transition"
          >
            ×
          </button>
        )}
      </motion.div>
    </AnimatePresence>
  );
}

/**
 * Format the counter line. For model-download events the totals are
 * in bytes (we want MB), for everything else they're item counts.
 */
function humanize(
  processed: number,
  total: number,
  phase: IndexingPhase,
): string {
  if (phase === "model-download") {
    return `${(processed / 1_048_576).toFixed(0)} / ${(total / 1_048_576).toFixed(0)} MB`;
  }
  return `${processed} / ${total}`;
}
