/**
 * Per-image free-text annotations (Phase 11). Backed by the
 * `notes` column on the images table; IPC commands are
 * `get_image_notes` / `set_image_notes`.
 *
 * Empty string at the user-facing level means "no annotation"; the
 * backend collapses NULL and "" into the same Option<String>::None.
 */
import { invoke } from "@tauri-apps/api/core";

export async function getImageNotes(imageId: number): Promise<string> {
  try {
    return await invoke<string>("get_image_notes", { imageId });
  } catch (error) {
    throw new Error(`Failed to get notes: ${error}`);
  }
}

export async function setImageNotes(
  imageId: number,
  notes: string,
): Promise<void> {
  try {
    await invoke("set_image_notes", { imageId, notes });
  } catch (error) {
    throw new Error(`Failed to save notes: ${error}`);
  }
}
