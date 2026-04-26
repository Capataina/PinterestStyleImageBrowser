import { useQuery } from "@tanstack/react-query";
import { semanticSearch } from "../services/images";
import { SimilarImageItem } from "../types";
import { useUserPreferences } from "../hooks/useUserPreferences";

/**
 * React Query hook for semantic search.
 *
 * Phase 4 — dispatches through the user's `textEncoder` preference
 * (CLIP English vs SigLIP-2 multilingual-ish). The encoder id flows
 * into the queryKey so a switch in the picker invalidates the cached
 * results — different encoders produce different result orderings,
 * stale cache from the previous encoder would be misleading.
 *
 * @param query - The search text
 * @param topN - Maximum number of results (default: 50)
 */
export function useSemanticSearch(query: string, topN: number = 50) {
  const trimmedQuery = query.trim();
  const { prefs } = useUserPreferences();
  const textEncoderId = prefs.textEncoder;

  return useQuery<SimilarImageItem[]>({
    queryKey: ["semantic-search", trimmedQuery, topN, textEncoderId],
    queryFn: () => semanticSearch(trimmedQuery, topN, textEncoderId),
    // Only run the query if there's actual search text
    enabled: trimmedQuery.length > 0,
    // Cache results for 5 minutes
    staleTime: 1000 * 60 * 5,
    // Keep in cache for 10 minutes
    gcTime: 1000 * 60 * 10,
    // Don't refetch on window focus for search results
    refetchOnWindowFocus: false,
  });
}
