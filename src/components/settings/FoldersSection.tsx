import { FolderPlus, Trash2 } from "lucide-react";
import {
  useAddRoot,
  useRemoveRoot,
  useRoots,
  useSetRootEnabled,
} from "../../queries/useRoots";
import { pickScanFolder } from "../../services/images";
import { recordAction } from "../../services/perf";
import { Section, Toggle } from "./controls";

export function FoldersSection() {
  const { data: roots } = useRoots();
  const addRootMutation = useAddRoot();
  const removeRootMutation = useRemoveRoot();
  const toggleRootMutation = useSetRootEnabled();

  return (
    <Section title="Folders">
      <p className="text-xs text-muted-foreground -mt-1">
        The app indexes every enabled folder recursively. Disable
        to exclude without losing the index; remove to delete the
        index entirely.
      </p>
      <div className="flex flex-col gap-2">
        {(roots ?? []).map((root) => (
          <div
            key={root.id}
            className="flex items-center gap-3 rounded-lg border border-border bg-secondary/40 px-3 py-2.5"
          >
            <Toggle
              checked={root.enabled}
              onChange={(enabled) => {
                recordAction("folder_toggle", {
                  id: root.id,
                  enabled,
                });
                toggleRootMutation.mutate({ id: root.id, enabled });
              }}
            />
            <div className="flex-1 min-w-0">
              <p
                className={[
                  "text-xs truncate",
                  root.enabled
                    ? "text-foreground"
                    : "text-muted-foreground",
                ].join(" ")}
                title={root.path}
              >
                {root.path}
              </p>
            </div>
            <button
              onClick={() => {
                if (
                  window.confirm(
                    `Remove ${root.path}?\n\nThe images from this folder will be removed from the index. The actual files on disk are not touched.`,
                  )
                ) {
                  recordAction("folder_remove", {
                    id: root.id,
                    path: root.path,
                  });
                  removeRootMutation.mutate(root.id);
                }
              }}
              aria-label="Remove folder"
              className="rounded p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}

        {(roots ?? []).length === 0 && (
          <p className="text-xs text-muted-foreground italic">
            No folders configured yet.
          </p>
        )}
      </div>

      <button
        className="flex items-center gap-2 rounded-lg bg-primary text-primary-foreground px-3 py-2 text-xs font-medium hover:opacity-90 transition w-full justify-center"
        onClick={async () => {
          try {
            const folder = await pickScanFolder();
            if (!folder) return;
            recordAction("folder_add", { path: folder });
            await addRootMutation.mutateAsync(folder);
          } catch (err) {
            window.alert(
              `Could not add folder: ${err instanceof Error ? err.message : String(err)}`,
            );
          }
        }}
      >
        <FolderPlus className="h-3.5 w-3.5" />
        Add folder
      </button>
    </Section>
  );
}
