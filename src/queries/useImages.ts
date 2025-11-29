import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ImageItem, Tag } from "../types";
import { assignTagToImage, fetchImages } from "../services/images";

export function useImages() {
  return useQuery<ImageItem[]>({
    queryKey: ["images"],
    queryFn: fetchImages,
    enabled: true,
  });
}

export function useAssignTagToImage() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params: { imageId: string; tagId: string }) =>
      assignTagToImage(params.imageId, params.tagId),

    onMutate: async (params) => {
      await queryClient.cancelQueries({ queryKey: ["images"] });

      const prevImages = queryClient.getQueryData(["images"]);

      // Get the tag from tags cache
      const tags = queryClient.getQueryData<Tag[]>(["tags"]);
      const tagToAdd = tags?.find((t) => t.id === params.tagId);

      // Optimistically update images
      if (tagToAdd) {
        queryClient.setQueryData<ImageItem[]>(["images"], (old = []) =>
          old.map((img) =>
            img.id === params.imageId
              ? { ...img, tags: [...img.tags, tagToAdd] }
              : img
          )
        );
      }

      return { prevImages };
    },

    onError: (_err, _vars, context) => {
      if (context?.prevImages) {
        queryClient.setQueryData(["images"], context.prevImages);
      }
    },

    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ["images"] });
    },
  });
}
