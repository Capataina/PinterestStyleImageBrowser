import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ImageData, ImageItem, SimilarImageItem } from "../types";
import { perfInvoke } from "./perf";

// Default dimensions if backend doesn't provide them (fallback)
const DEFAULT_WIDTH = 800;
const DEFAULT_HEIGHT = 600;

/** Frontend-side sort modes. Backend always returns stable order
 *  (by id ASC); we apply name/added/shuffle here. The shuffle uses
 *  a seed argument so refetches with the same seed yield the same
 *  order — only when the seed changes does the order change. */
export type SortMode = "id" | "name" | "added" | "shuffle";

export async function fetchImages(
  filterTagIds: number[] = [],
  filterString: string = "",
  matchAllTags: boolean = false,
  sortMode: SortMode = "id",
  shuffleSeed: number = 0,
): Promise<ImageItem[]> {
  try {
    const imagesDB: ImageData[] = await perfInvoke("get_images", {
      filterTagIds,
      filterString,
      matchAllTags,
    });

    // Convert backend data to frontend ImageItem format
    // Now using dimensions from backend (stored during thumbnail generation)
    const images: ImageItem[] = imagesDB.map((img) => {
      const url = convertFileSrc(img.path);
      // Use thumbnail URL if available, otherwise fall back to full image
      const thumbnailUrl = img.thumbnail_path
        ? convertFileSrc(img.thumbnail_path)
        : url;

      return {
        id: img.id,
        name: img.name,
        url,
        thumbnailUrl,
        // Use dimensions from backend, or defaults if not available
        width: img.width ?? DEFAULT_WIDTH,
        height: img.height ?? DEFAULT_HEIGHT,
        tags: img.tags,
      };
    });

    // Apply sort mode frontend-side. Backend returned stable order
    // by id; we re-order here as the user prefers.
    return applySortMode(images, sortMode, shuffleSeed);
  } catch (error) {
    throw new Error(`Failed to fetch images: ${error}`);
  }
}

/**
 * Apply the user's preferred sort to a list of images.
 *
 * Critically, "shuffle" uses a deterministic seeded shuffle: the
 * same input + same seed always produces the same output. The
 * frontend controls when the seed changes (e.g. on modal close, on
 * pull-to-refresh, or on a deliberate "shuffle again" button), so
 * progressive thumbnail loading during indexing doesn't keep
 * re-rolling the order mid-session.
 */
export function applySortMode(
  items: ImageItem[],
  mode: SortMode,
  seed: number,
): ImageItem[] {
  // Don't mutate the input — return a copy so React state changes
  // are detected via reference inequality.
  const out = items.slice();
  switch (mode) {
    case "id":
    case "added":
      // Backend already returns ASC by id. "added" is the same thing
      // because newer rows get higher ids.
      out.sort((a, b) => a.id - b.id);
      return out;
    case "name":
      out.sort((a, b) => a.name.localeCompare(b.name));
      return out;
    case "shuffle": {
      if (seed === 0) {
        // Seed 0 = "no shuffle yet". Return stable order so the
        // grid doesn't reshuffle on a session-fresh load until the
        // user explicitly refreshes.
        out.sort((a, b) => a.id - b.id);
        return out;
      }
      // Mulberry32 — small, fast, seedable PRNG. Same seed → same
      // output. Used here for Fisher-Yates shuffle.
      let s = seed >>> 0;
      const rand = () => {
        s = (s + 0x6d2b79f5) >>> 0;
        let t = s;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
      };
      for (let i = out.length - 1; i > 0; i--) {
        const j = Math.floor(rand() * (i + 1));
        [out[i], out[j]] = [out[j], out[i]];
      }
      return out;
    }
  }
}

export async function assignTagToImage(
  imageId: number,
  tagId: number
): Promise<void> {
  try {
    await invoke("add_tag_to_image", {
      imageId,
      tagId,
    });
  } catch (error) {
    console.error(`Failed to assign tag: ${error}`);
    throw new Error(`Failed to assign tag: ${error}`);
  }
}

export async function removeTagFromImage(
  imageId: number,
  tagId: number
): Promise<void> {
  try {
    await invoke("remove_tag_from_image", {
      imageId,
      tagId,
    });
  } catch (error) {
    console.error(`Failed to remove tag: ${error}`);
    throw new Error(`Failed to remove tag: ${error}`);
  }
}

// Helper to construct thumbnail path from image ID.
// Backend stores thumbnails inside app_data_dir()/thumbnails/ but the
// canonical path comes back on the ImageData / SemanticSearchResult
// objects, so this function is only a fallback when the backend didn't
// supply one (very old DB rows pre-migration). The exact filename
// pattern still matches `thumb_{id}.jpg` as defined in
// src-tauri/src/thumbnail/generator.rs.
function getThumbnailPath(imageId: number): string {
  return `thumbnails/thumb_${imageId}.jpg`;
}

/**
 * Open a native folder picker and return the selected directory path.
 * Returns null if the user cancelled the dialog.
 */
export async function pickScanFolder(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Choose your image folder",
  });
  // open() returns null on cancel, a string for single selection, or an
  // array for multiple — we forced multiple: false so it's string | null.
  if (typeof selected === "string") return selected;
  return null;
}

export async function getScanRoot(): Promise<string | null> {
  try {
    return (await invoke<string | null>("get_scan_root")) ?? null;
  } catch (error) {
    throw new Error(`Failed to read scan root: ${error}`);
  }
}

/**
 * Persist the chosen scan root and wipe the existing image index.
 *
 * Pass 4a behaviour: the backend persists the path and clears image
 * rows. Re-indexing happens on the next app launch — Pass 5 will
 * trigger it live and emit progress events.
 */
export async function setScanRoot(path: string): Promise<void> {
  try {
    await invoke("set_scan_root", { path });
  } catch (error) {
    throw new Error(`Failed to set scan root: ${error}`);
  }
}

/**
 * Map a backend ImageSearchResult into the frontend's SimilarImageItem
 * shape. All three search commands (semantic, similar, tiered) now
 * return the same struct (audit consolidation), so this single helper
 * covers every call site.
 *
 * Backend supplies thumbnail_path/width/height when the image was
 * thumbnailed at indexing time. Legacy DB rows may lack them; fall
 * back to the canonical thumb_{id}.jpg path + default dimensions.
 */
function mapImageSearchResult(res: {
  id: number;
  path: string;
  score: number;
  thumbnail_path?: string;
  width?: number;
  height?: number;
}): SimilarImageItem {
  return {
    id: res.id,
    path: res.path,
    url: convertFileSrc(res.path),
    thumbnailUrl: res.thumbnail_path
      ? convertFileSrc(res.thumbnail_path)
      : convertFileSrc(getThumbnailPath(res.id)),
    width: res.width ?? DEFAULT_WIDTH,
    height: res.height ?? DEFAULT_HEIGHT,
    score: res.score,
    name: res.path.split(/[\\/]/).pop() ?? res.path,
  };
}

export async function fetchSimilarImages(imageId: number, topN: number = 8) {
  try {
    // Backend now returns the unified ImageSearchResult shape
    // (thumbnail_path/width/height included). Audit Performance
    // finding: dimensions used to be fetched frontend-side via
    // N parallel `getImageSize` DOM image loads — gone now,
    // a single IPC round-trip carries the full payload.
    const results: Parameters<typeof mapImageSearchResult>[0][] = await perfInvoke(
      "get_similar_images",
      { imageId, topN }
    );
    return results.map(mapImageSearchResult);
  } catch (error) {
    console.error("[Frontend] Error in fetchSimilarImages:", error);
    throw new Error(`Failed to fetch similar images: ${error}`);
  }
}

export async function fetchTieredSimilarImages(imageId: number) {
  try {
    const results: Parameters<typeof mapImageSearchResult>[0][] = await perfInvoke(
      "get_tiered_similar_images",
      { imageId }
    );
    return results.map(mapImageSearchResult);
  } catch (error) {
    console.error("[Frontend] Error in fetchTieredSimilarImages:", error);
    throw new Error(`Failed to fetch tiered similar images: ${error}`);
  }
}

/**
 * Semantic search: find images matching a text query using CLIP embeddings.
 * Supports 50+ languages thanks to the multilingual CLIP model.
 *
 * @param query - The search text (e.g., "blue ocean", "dog playing", "夕焼け")
 * @param topN - Maximum number of results to return (default: 50)
 */
export async function semanticSearch(
  query: string,
  topN: number = 50
): Promise<SimilarImageItem[]> {
  try {
    const results: Parameters<typeof mapImageSearchResult>[0][] = await perfInvoke(
      "semantic_search",
      { query, topN }
    );
    return results.map(mapImageSearchResult);
  } catch (error) {
    console.error("[Frontend] Error in semanticSearch:", error);
    throw new Error(`Semantic search failed: ${error}`);
  }
}