/**
 * Multi-folder root management — IPC wrappers for the Tauri commands
 * defined in src-tauri/src/lib.rs (list_roots, add_root, remove_root,
 * set_root_enabled).
 *
 * The settings drawer (Phase 9) renders the list of configured roots
 * with toggle / remove controls and an "add folder" button.
 */
import { invoke } from "@tauri-apps/api/core";
import { Root } from "../types";

export async function listRoots(): Promise<Root[]> {
  try {
    return await invoke<Root[]>("list_roots");
  } catch (error) {
    throw new Error(`Failed to list roots: ${error}`);
  }
}

/**
 * Add a folder as a new root and trigger an incremental re-index.
 * The backend returns the populated Root row so the UI can render
 * it instantly without waiting for a list_roots round-trip.
 */
export async function addRoot(path: string): Promise<Root> {
  try {
    return await invoke<Root>("add_root", { path });
  } catch (error) {
    throw new Error(`Failed to add root: ${error}`);
  }
}

/**
 * Remove a root and CASCADE-delete its image rows from the DB. Tags
 * survive — they're catalogue-level user knowledge.
 */
export async function removeRoot(id: number): Promise<void> {
  try {
    await invoke("remove_root", { id });
  } catch (error) {
    throw new Error(`Failed to remove root: ${error}`);
  }
}

/**
 * Toggle a root's enabled flag. Disabled roots keep their image rows
 * on disk (re-enabling is instant) but are filtered out of the grid.
 * No re-index is triggered — the change is purely a query filter.
 */
export async function setRootEnabled(
  id: number,
  enabled: boolean,
): Promise<void> {
  try {
    await invoke("set_root_enabled", { id, enabled });
  } catch (error) {
    throw new Error(`Failed to toggle root: ${error}`);
  }
}
