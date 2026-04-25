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
 * - When `phase` is "thumbnail" or "encode", periodically invalidates
 *   the ["images"] query so the grid refreshes as new images appear.
 *   Throttled — at most once every 2 seconds — to avoid render storms
 *   on large libraries (10k+ images).
 * - When `phase` reaches "ready", invalidates one final time so the
 *   grid shows the complete catalog.
 */
export function useIndexingProgress(): IndexingState {
  const [progress, setProgress] = useState<IndexingProgress | null>(null);
  const queryClient = useQueryClient();

  // Throttle marker for invalidateQueries during long phases.
  const lastInvalidatedAt = useRef<number>(0);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    (async () => {
      const off = await listen<IndexingProgress>(
        "indexing-progress",
        (event) => {
          const payload = event.payload;
          setProgress(payload);

          // Throttled image-cache invalidation on the long phases.
          if (payload.phase === "thumbnail" || payload.phase === "encode") {
            const now = Date.now();
            if (now - lastInvalidatedAt.current > 2000) {
              lastInvalidatedAt.current = now;
              queryClient.invalidateQueries({ queryKey: ["images"] });
            }
          } else if (payload.phase === "ready") {
            queryClient.invalidateQueries({ queryKey: ["images"] });
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
