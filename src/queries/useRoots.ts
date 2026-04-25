import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { Root } from "../types";
import {
  addRoot,
  listRoots,
  removeRoot,
  setRootEnabled,
} from "@/services/roots";

/**
 * TanStack Query hooks for the multi-folder root management surface.
 * The settings drawer renders the list and the toggle/remove controls
 * via these.
 *
 * Mutations follow the canonical optimistic shape (cancelQueries ->
 * snapshot -> setQueriesData -> rollback on error) — same pattern as
 * useImages and useTags.
 */
export function useRoots() {
  return useQuery<Root[]>({
    queryKey: ["roots"],
    queryFn: listRoots,
    enabled: true,
  });
}

export function useAddRoot() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string) => addRoot(path),
    onSuccess: (newRoot) => {
      qc.setQueryData<Root[]>(["roots"], (old = []) => [...old, newRoot]);
      // The backend triggers a re-index on its own; the indexing-progress
      // event channel will surface the new tiles. We invalidate the
      // image query proactively so the empty-state vanishes once the
      // first thumbnails land.
      qc.invalidateQueries({ queryKey: ["images"] });
    },
  });
}

export function useRemoveRoot() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => removeRoot(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: ["roots"] });
      const prevRoots = qc.getQueryData<Root[]>(["roots"]);
      qc.setQueryData<Root[]>(["roots"], (old = []) =>
        old.filter((r) => r.id !== id),
      );
      return { prevRoots };
    },
    onError: (_err, _id, ctx) => {
      if (ctx?.prevRoots) {
        qc.setQueryData(["roots"], ctx.prevRoots);
      }
    },
    onSuccess: () => {
      // Image set changed — refetch so the grid drops the removed root's
      // images.
      qc.invalidateQueries({ queryKey: ["images"] });
    },
  });
}

export function useSetRootEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      setRootEnabled(id, enabled),
    onMutate: async ({ id, enabled }) => {
      await qc.cancelQueries({ queryKey: ["roots"] });
      const prevRoots = qc.getQueryData<Root[]>(["roots"]);
      qc.setQueryData<Root[]>(["roots"], (old = []) =>
        old.map((r) => (r.id === id ? { ...r, enabled } : r)),
      );
      return { prevRoots };
    },
    onError: (_err, _vars, ctx) => {
      if (ctx?.prevRoots) {
        qc.setQueryData(["roots"], ctx.prevRoots);
      }
    },
    onSuccess: () => {
      // Toggling enabled changes which images the grid query returns.
      qc.invalidateQueries({ queryKey: ["images"] });
    },
  });
}
