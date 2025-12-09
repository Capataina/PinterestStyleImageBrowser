import { useQuery } from "@tanstack/react-query";
import { fetchSimilarImages } from "../services/images";
import { SimilarImageItem } from "../types";

export function useSimilarImages(imageId?: number, topN: number = 8) {
  return useQuery<SimilarImageItem[]>({
    queryKey: ["similar-images", imageId, topN],
    queryFn: () => fetchSimilarImages(imageId!, topN),
    enabled: !!imageId,
  });
}

