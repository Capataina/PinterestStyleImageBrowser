import { useQuery } from "@tanstack/react-query";
import { fetchTieredSimilarImages } from "../services/images";
import { SimilarImageItem } from "../types";

export function useTieredSimilarImages(imageId?: number, encoderId?: string) {
  return useQuery<SimilarImageItem[]>({
    // encoderId in the queryKey means switching encoders in Settings
    // automatically refetches (stale-while-revalidate) — the user
    // sees results from the new encoder without manual refresh.
    queryKey: ["tiered-similar-images", imageId, encoderId],
    queryFn: () => fetchTieredSimilarImages(imageId!, encoderId),
    enabled: !!imageId,
  });
}

