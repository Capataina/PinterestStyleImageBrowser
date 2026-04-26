import { useQuery } from "@tanstack/react-query";
import { fetchFusedSimilarImages } from "../services/images";
import { SimilarImageItem } from "../types";

/**
 * Phase 5 — image-image similarity, now backed by multi-encoder rank
 * fusion (Reciprocal Rank Fusion across CLIP + SigLIP-2 + DINOv2).
 *
 * The hook keeps its `useTieredSimilarImages` name for caller stability
 * — every consumer that imports it (PinterestModal, etc.) gets fusion
 * behind the same surface without a wave of import renames. The
 * `encoderId` arg is now a hint only; fusion uses every available
 * encoder regardless of which one the user "selected" as their
 * primary. The id still flows into the queryKey because switching
 * the picker invalidates (the priority encoder change might trigger
 * a re-encode on the next pipeline run, which would in turn change
 * fusion's input).
 *
 * `topN` defaults to 30 — chosen empirically as a reasonable masonry
 * grid size. Bump it if the user's modal needs more results.
 */
export function useTieredSimilarImages(
  imageId?: number,
  encoderId?: string,
  topN: number = 30
) {
  return useQuery<SimilarImageItem[]>({
    queryKey: ["fused-similar-images", imageId, encoderId, topN],
    queryFn: () => fetchFusedSimilarImages(imageId!, topN),
    enabled: !!imageId,
  });
}

