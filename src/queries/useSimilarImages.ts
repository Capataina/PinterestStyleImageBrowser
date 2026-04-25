import { useQuery } from "@tanstack/react-query";
import { fetchTieredSimilarImages } from "../services/images";
import { SimilarImageItem } from "../types";

export function useTieredSimilarImages(imageId?: number) {
  return useQuery<SimilarImageItem[]>({
    queryKey: ["tiered-similar-images", imageId],
    queryFn: () => fetchTieredSimilarImages(imageId!),
    enabled: !!imageId,
  });
}

