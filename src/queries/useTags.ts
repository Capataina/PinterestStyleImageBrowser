import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Tag } from "../types";
import { createTag, fetchTags } from "@/services/tags";

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
