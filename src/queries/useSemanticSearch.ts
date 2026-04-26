import { useQuery } from "@tanstack/react-query";
import { fetchFusedSemanticSearch } from "../services/images";
import { SimilarImageItem } from "../types";

/**
 * Phase 11d — text-image search routes through multi-encoder rank
 * fusion across every enabled text-supporting encoder.
 *
 * The hook keeps its previous name so existing call sites
 * (`pages/[...slug].tsx` etc.) don't change shape. The implementation
 * now calls `get_fused_semantic_search`, which reads
 * settings.json::enabled_encoders to pick which text encoders
 * contribute. The user controls that list via the EncoderSection
 * toggles in the Settings drawer.
 *
 * Why no encoder id in the queryKey: the enabled set is process-
 * global (lives in settings.json on the backend). Toggling an
 * encoder doesn't dirty React's per-hook cache — but the fused
 * results WILL differ, so we'd want an invalidation signal. Today
 * that signal would arrive via app restart or a future
 * "encoders-changed" event. For now, users may need to retype the
 * query after toggling encoders to see the new fusion. Acceptable
 * trade-off; the enabled toggles are not high-frequency mutations.
 */
export function useSemanticSearch(query: string, topN: number = 50) {
  const trimmedQuery = query.trim();

  return useQuery<SimilarImageItem[]>({
    queryKey: ["fused-semantic-search", trimmedQuery, topN],
    queryFn: () => fetchFusedSemanticSearch(trimmedQuery, topN),
    enabled: trimmedQuery.length > 0,
    staleTime: 1000 * 60 * 5,
    gcTime: 1000 * 60 * 10,
    refetchOnWindowFocus: false,
  });
}
