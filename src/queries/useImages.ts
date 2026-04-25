import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ImageItem, Tag } from "../types";
import {
  assignTagToImage,
  fetchImages,
  removeTagFromImage,
  type SortMode,
} from "../services/images";

export function useImages(filters?: {
  tagIds?: number[];
  searchText?: string;
  /** When true, match images that have ALL selected tags (AND).
   *  When false (default), match images with ANY selected tag (OR). */
  matchAllTags?: boolean;
  /** User's preferred sort. Defaults to "id" (stable). */
  sortMode?: SortMode;
  /** Seed for shuffle mode. Same seed always produces the same order.
   *  Bumped only on deliberate refresh actions, not on indexing-progress
   *  invalidations — that way progressive thumbnail loading doesn't
   *  cause the grid to reshuffle every couple of seconds. */
  shuffleSeed?: number;
}) {
  const tagIds = filters?.tagIds ?? [];
  // searchText is intentionally NOT in the queryKey: the backend ignores
  // it (filtering happens via tagIds + the separate semantic_search command),
  // so keying on it would produce a cache miss per keystroke for identical
  // data. We still pass it through to fetchImages for future-proofing.
  const searchText = filters?.searchText ?? "";
  const matchAllTags = filters?.matchAllTags ?? false;
  const sortMode = filters?.sortMode ?? "id";
  const shuffleSeed = filters?.shuffleSeed ?? 0;

  return useQuery<ImageItem[]>({
    // Include sortMode + shuffleSeed in the key so a sort change or
    // a deliberate reshuffle invalidates the cache. Indexing-progress
    // invalidates with the SAME key, which means the seed stays
    // constant and the order stays stable.
    queryKey: ["images", tagIds, matchAllTags, sortMode, shuffleSeed],
    queryFn: () =>
      fetchImages(tagIds, searchText, matchAllTags, sortMode, shuffleSeed),
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
