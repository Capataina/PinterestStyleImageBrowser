import { useQuery } from "@tanstack/react-query";
import { semanticSearch } from "../services/images";
import { SimilarImageItem } from "../types";

/**
 * React Query hook for semantic search.
 * Searches images by text using CLIP embeddings.
 *
 * @param query - The search text (supports 50+ languages)
 * @param topN - Maximum number of results (default: 50)
 */
export function useSemanticSearch(query: string, topN: number = 50) {
  const trimmedQuery = query.trim();

  return useQuery<SimilarImageItem[]>({
    queryKey: ["semantic-search", trimmedQuery, topN],
    queryFn: () => semanticSearch(trimmedQuery, topN),
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
