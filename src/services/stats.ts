/**
 * Pipeline progress stats — counts of images at each stage.
 *
 * Returned by the `get_pipeline_stats` Tauri command. Drives the
 * "Indexing Progress" section in SettingsDrawer so the user can see
 * how much work the indexing pipeline has done at a glance.
 *
 * Mirrors the backend `PipelineStats` struct in
 * src-tauri/src/db/images_query.rs.
 */
import { invoke } from "@tauri-apps/api/core";
import { formatApiError } from "./apiError";

export interface PipelineStats {
  total_images: number;
  with_thumbnail: number;
  with_embedding: number;
  orphaned: number;
}

export async function getPipelineStats(): Promise<PipelineStats> {
  try {
    return await invoke<PipelineStats>("get_pipeline_stats");
  } catch (error) {
    throw new Error(formatApiError(error));
  }
}
