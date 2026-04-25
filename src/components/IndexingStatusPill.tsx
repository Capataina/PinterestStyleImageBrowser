import { motion, AnimatePresence } from "framer-motion";
import { Loader2, AlertCircle, CheckCircle2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  useIndexingProgress,
  type IndexingPhase,
} from "../hooks/useIndexingProgress";

const PHASE_LABELS: Record<IndexingPhase, string> = {
  scan: "Scanning folder",
  "model-download": "Downloading models",
  thumbnail: "Generating thumbnails",
  encode: "Encoding embeddings",
  ready: "Ready",
  error: "Error",
};

/**
 * Floating status pill that lives in the top-right corner of the page
 * and reports indexing pipeline progress.
 *
 * Visibility:
 * - Hidden until the first event arrives (so a fresh launch with no
 *   scan_root doesn't flash a pill).
 * - Shown while phase is anything except "ready" / "error".
 * - "ready" stays visible for 4s as a "X images indexed" confirmation,
 *   then auto-dismisses.
 * - "error" stays visible until the user dismisses (via the × button)
 *   or until a new pipeline run starts.
 */
export function IndexingStatusPill() {
  // We deliberately don't bind isIndexing here — visibility logic uses
  // progress.phase directly because we want different rules for ready
  // (auto-dismiss after 4s) vs error (stay until manually dismissed).
  const { progress } = useIndexingProgress();
  const [dismissed, setDismissed] = useState(false);
  const [showFinal, setShowFinal] = useState(false);

  // When the pipeline goes ready, briefly show "X images indexed" then
  // fade out. Resets dismissed state when a new run begins.
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
    // Any non-terminal phase resets dismissal — the user dismissed an
    // earlier error, but we're in flight again now.
    setDismissed(false);
    setShowFinal(false);
  }, [progress?.phase]); // eslint-disable-line react-hooks/exhaustive-deps

  // Visibility decision:
  if (!progress) return null;
  if (dismissed) return null;
  if (progress.phase === "ready" && !showFinal) return null;

  const isError = progress.phase === "error";
  const isReady = progress.phase === "ready";
  const label = PHASE_LABELS[progress.phase];

  // Determinate progress bar fill (0..1). Indeterminate phases (no
  // total) leave the bar empty and show only the spinner.
  const fill =
    progress.total > 0 ? Math.min(1, progress.processed / progress.total) : 0;
  const showBar = progress.total > 0 && !isError && !isReady;

  return (
    <AnimatePresence>
      <motion.div
        key="indexing-pill"
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -10 }}
        transition={{ duration: 0.2 }}
        className={[
          "fixed top-4 right-4 z-50 flex items-center gap-3",
          "rounded-full border bg-white/95 backdrop-blur-sm",
          "px-4 py-2 shadow-lg",
          "min-w-[260px] max-w-[400px]",
          isError ? "border-red-200" : "border-gray-200",
        ].join(" ")}
      >
        <div className="shrink-0">
          {isError ? (
            <AlertCircle className="h-5 w-5 text-red-500" />
          ) : isReady ? (
            <CheckCircle2 className="h-5 w-5 text-green-500" />
          ) : (
            <Loader2 className="h-5 w-5 animate-spin text-gray-600" />
          )}
        </div>

        <div className="flex flex-1 flex-col gap-1 overflow-hidden">
          <div className="flex items-center justify-between gap-2">
            <span
              className={[
                "text-sm font-medium",
                isError ? "text-red-700" : "text-gray-800",
              ].join(" ")}
            >
              {label}
            </span>
            {progress.total > 0 && !isError && !isReady && (
              <span className="text-xs tabular-nums text-gray-500">
                {progress.processed} / {progress.total}
              </span>
            )}
          </div>

          {showBar && (
            <div className="h-1 w-full overflow-hidden rounded-full bg-gray-100">
              <motion.div
                className="h-full bg-gray-700"
                initial={false}
                animate={{ width: `${fill * 100}%` }}
                transition={{ duration: 0.25 }}
              />
            </div>
          )}

          {progress.message && !isReady && (
            <span
              className={[
                "truncate text-xs",
                isError ? "text-red-600" : "text-gray-500",
              ].join(" ")}
              title={progress.message}
            >
              {progress.message}
            </span>
          )}

          {isReady && progress.message && (
            <span className="text-xs text-gray-500">{progress.message}</span>
          )}
        </div>

        {(isError || isReady) && (
          <button
            type="button"
            onClick={() => setDismissed(true)}
            aria-label="Dismiss"
            className="shrink-0 rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700"
          >
            ×
          </button>
        )}
      </motion.div>
    </AnimatePresence>
  );
}
