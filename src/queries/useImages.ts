import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ImageItem, Tag } from "../types";
import {
  assignTagToImage,
  fetchImages,
  removeTagFromImage,
} from "../services/images";

export function useImages(filters?: {
  tagIds?: number[];
  searchText?: string;
}) {
  const tagIds = filters?.tagIds ?? [];
  const searchText = filters?.searchText ?? "";

  return useQuery<ImageItem[]>({
    queryKey: ["images", tagIds, searchText],
    queryFn: () => fetchImages(tagIds, searchText),
    enabled: true,
  });
}

export function useAssignTagToImage() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params: { imageId: number; tagId: number }) =>
      assignTagToImage(params.imageId, params.tagId),

    onMutate: async (params) => {
      await queryClient.cancelQueries({ queryKey: ["images"] });

      const prevImages = queryClient.getQueryData(["images"]);

      // Get the tag from tags cache
      const tags = queryClient.getQueryData<Tag[]>(["tags"]);
      const tagToAdd = tags?.find((t) => t.id === params.tagId);

      console.log(params.tagId);

      // Optimistically update images
      if (tagToAdd) {
        queryClient.setQueriesData<ImageItem[]>(
          { queryKey: ["images"], exact: false },
          (old = []) =>
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
  });
}

export function useRemoveTagFromImage() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params: { imageId: number; tagId: number }) =>
      removeTagFromImage(params.imageId, params.tagId),

    onMutate: async (params) => {
      await queryClient.cancelQueries({ queryKey: ["images"] });

      const prevImages = queryClient.getQueryData(["images"]);

      // Optimistically remove the tag from the image
      queryClient.setQueriesData<ImageItem[]>(
        { queryKey: ["images"], exact: false },
        (old = []) =>
          old.map((img) =>
            img.id === params.imageId
              ? { ...img, tags: img.tags.filter((t) => t.id !== params.tagId) }
              : img
          )
      );

      return { prevImages };
    },

    onError: (_err, _vars, context) => {
      if (context?.prevImages) {
        queryClient.setQueryData(["images"], context.prevImages);
      }
    },
  });
}
