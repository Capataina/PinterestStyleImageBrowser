import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ImageItem, Tag } from "../types";
import { createTag, deleteTag, fetchTags } from "@/services/tags";

export function useTags() {
  return useQuery<Tag[]>({
    queryKey: ["tags"],
    queryFn: fetchTags,
    enabled: true,
  });
}

export function useCreateTag() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params: { name: string; color: string }) =>
      createTag(params.name, params.color),

    onMutate: async (newTag) => {
      await queryClient.cancelQueries({ queryKey: ["tags"] });

      const prevTags = queryClient.getQueryData(["tags"]);

      queryClient.setQueryData(["tags"], (old: Tag[] = []) => [
        ...old,
        { ...newTag, id: -1, optimistic: true },
      ]);

      return { prevTags };
    },

    onError: (_err, _newTodo, context) => {
      if (context?.prevTags) {
        queryClient.setQueryData(["tags"], context.prevTags);
      }
    },

    onSuccess: (newTag) => {
      queryClient.setQueryData<Tag[]>(["tags"], (old = []) =>
        old.map((tag) => (tag.id === -1 ? newTag : tag))
      );
    },
  });
}

export function useDeleteTag() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (tagId: number) => deleteTag(tagId),

    onMutate: async (tagId) => {
      await queryClient.cancelQueries({ queryKey: ["tags"] });
      await queryClient.cancelQueries({ queryKey: ["images"] });

      const prevTags = queryClient.getQueryData<Tag[]>(["tags"]);

      // Optimistically remove from tags catalog
      queryClient.setQueryData<Tag[]>(["tags"], (old = []) =>
        old.filter((t) => t.id !== tagId)
      );

      // Also strip the tag from any image that has it (the DB does this
      // via ON DELETE CASCADE on images_tags; we mirror it in the cache so
      // the UI doesn't show ghost tags until the next refetch).
      queryClient.setQueriesData<ImageItem[]>(
        { queryKey: ["images"], exact: false },
        (old = []) =>
          old.map((img) => ({
            ...img,
            tags: img.tags.filter((t) => t.id !== tagId),
          }))
      );

      return { prevTags };
    },

    onError: (_err, _vars, context) => {
      if (context?.prevTags) {
        queryClient.setQueryData(["tags"], context.prevTags);
      }
      // Image cache will self-correct on next refetch; we don't snapshot
      // every image query because there can be many cache keys.
      queryClient.invalidateQueries({ queryKey: ["images"] });
    },
  });
}
