import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";

/**
 * Indexing pipeline phases — matches the kebab-case Phase enum in
 * src-tauri/src/indexing.rs.
 */
export type IndexingPhase =
  | "scan"
  | "model-download"
  | "thumbnail"
  | "encode"
  | "ready"
  | "error";

/**
 * Event payload emitted by the backend on `indexing-progress`. Mirrors
 * the IndexingProgress struct in indexing.rs — see the comment block
 * at the top of that file for the source of truth.
 */
export interface IndexingProgress {
  phase: IndexingPhase;
  processed: number;
  /** Zero means "indeterminate" (e.g. "model-download" with no content-length). */
  total: number;
  message: string | null;
}

/**
 * Shape returned to UI consumers. Adds a couple of derived fields the
 * status pill needs without re-deriving them at every render.
 */
export interface IndexingState {
  /** The latest progress event, or null before any event has arrived. */
  progress: IndexingProgress | null;
  /**
   * True while the pipeline is actively running. Goes false on `ready`
   * or `error`. Pill should hide when this is false (with a brief grace
   * period to display the final state).
   */
  isIndexing: boolean;
}

/**
 * Listens to the backend `indexing-progress` events and exposes the
 * current pipeline state to React.
 *
 * Side effects:
 * - During the "thumbnail" phase, periodically (every 5s) invalidates
 *   the ["images"] query so the grid refreshes as new thumbnails
 *   land. This is a real visible change worth refreshing for.
 * - During the "encode" phase we do NOT invalidate. Encoders only
 *   populate search-readiness (cosine cache), not the visible grid.
 *   Refreshing here drove the symptom diagnosed in
 *   `context/plans/performance-analysis.md`: full `get_images`
 *   round-trips contending with SigLIP-2 inference, producing two
 *   ~22-second freezes in a 1842-image folder add.
 * - On the "ready" phase transition, invalidate ["images"] ONCE so
 *   any final metadata (orphan flagging, root reconciliation) lands.
 */
export function useIndexingProgress(): IndexingState {
  const [progress, setProgress] = useState<IndexingProgress | null>(null);
  const queryClient = useQueryClient();

  // Throttle marker for invalidateQueries during the thumbnail phase.
  const lastInvalidatedAt = useRef<number>(0);
  // Track whether we've already done the one-shot ready invalidation
  // for the current pipeline run. The pipeline can re-emit `ready`
  // (e.g. after background DINOv2 finishes) — we only need one.
  const readyInvalidatedFor = useRef<string | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    (async () => {
      const off = await listen<IndexingProgress>(
        "indexing-progress",
        (event) => {
          const payload = event.payload;
          setProgress(payload);

          // Cache-invalidation strategy:
          //
          // Thumbnail phase: refresh every ~5s so newly-generated
          // thumbnails appear in the grid as they land. This is the
          // user-visible payoff of indexing — keep it lively.
          //
          // Encode phase: deliberately do NOT invalidate. Encoders
          // populate search-readiness, not the visible grid. The grid
          // was already correct after the thumbnail phase. Refreshing
          // during encode was the headline finding in
          // context/plans/performance-analysis.md — full grid refetches
          // contended with heavy SigLIP-2 inference and produced two
          // 22-second `get_images` stalls.
          //
          // Ready phase: one final invalidation so any metadata that
          // changed during the run (orphan flags, root cleanup) lands.
          if (payload.phase === "thumbnail") {
            const now = Date.now();
            if (now - lastInvalidatedAt.current > 5000) {
              lastInvalidatedAt.current = now;
              queryClient.invalidateQueries({ queryKey: ["images"] });
            }
          } else if (payload.phase === "ready") {
            // Reset the throttle so the next run's thumbnail phase
            // starts fresh.
            lastInvalidatedAt.current = 0;
            // De-dupe in case the pipeline re-emits `ready`. Use the
            // message field as a coarse run identifier — different
            // ready events for the same pipeline carry the same
            // message, distinct runs (next folder add) emit a fresh
            // sequence so this naturally re-arms.
            const runKey = payload.message ?? "ready";
            if (readyInvalidatedFor.current !== runKey) {
              readyInvalidatedFor.current = runKey;
              queryClient.invalidateQueries({ queryKey: ["images"] });
            }
          } else if (payload.phase === "scan") {
            // New run starting — re-arm the ready de-dupe.
            readyInvalidatedFor.current = null;
          }
        }
      );
      if (cancelled) {
        // Component unmounted between awaiting and resolving; clean up
        // immediately rather than leaking the listener.
        off();
      } else {
        unlisten = off;
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [queryClient]);

  const isIndexing =
    progress !== null &&
    progress.phase !== "ready" &&
    progress.phase !== "error";

  return { progress, isIndexing };
}
