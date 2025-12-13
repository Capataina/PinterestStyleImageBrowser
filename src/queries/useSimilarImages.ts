import { useQuery } from "@tanstack/react-query";
import { fetchSimilarImages, fetchTieredSimilarImages } from "../services/images";
import { SimilarImageItem } from "../types";

export function useSimilarImages(imageId?: number, topN: number = 8) {
  console.log("[React Query] useSimilarImages hook called", { imageId, topN, enabled: !!imageId });
  
  return useQuery<SimilarImageItem[]>({
    queryKey: ["similar-images", imageId, topN],
    queryFn: () => {
      console.log("[React Query] Query function executing for imageId:", imageId);
      return fetchSimilarImages(imageId!, topN);
    },
    enabled: !!imageId,
    onSuccess: (data) => {
      console.log("[React Query] Query success - received data:", {
        count: data?.length || 0,
        items: data?.map(item => ({ id: item.id, name: item.name, score: item.score })) || []
      });
    },
    onError: (error) => {
      console.error("[React Query] Query error:", error);
    },
  });
}

export function useTieredSimilarImages(imageId?: number) {
  return useQuery<SimilarImageItem[]>({
    queryKey: ["tiered-similar-images", imageId],
    queryFn: () => fetchTieredSimilarImages(imageId!),
    enabled: !!imageId,
  });
}

