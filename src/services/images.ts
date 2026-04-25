import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getImageSize } from "../utils";
import { ImageData, ImageItem, SimilarImageItem } from "../types";

// Default dimensions if backend doesn't provide them (fallback)
const DEFAULT_WIDTH = 800;
const DEFAULT_HEIGHT = 600;

export async function fetchImages(
  filterTagIds: number[] = [],
  filterString: string = "",
  matchAllTags: boolean = false,
): Promise<ImageItem[]> {
  try {
    const imagesDB: ImageData[] = await invoke("get_images", {
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

    return images;
  } catch (error) {
    throw new Error(`Failed to fetch images: ${error}`);
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

export async function fetchSimilarImages(imageId: number, topN: number = 8) {
  try {
    const results: { id: number; path: string; score: number }[] = await invoke(
      "get_similar_images",
      {
        imageId,
        topN,
      }
    );

    // For similar images, we need to get dimensions
    // We'll load them in parallel but with a concurrency limit
    const images = await Promise.all(
      results.map(async (res) => {
        const url = convertFileSrc(res.path);
        const thumbnailPath = getThumbnailPath(res.id);
        const thumbnailUrl = convertFileSrc(thumbnailPath);

        // Try to get dimensions from thumbnail (faster) or fall back to full image
        let width = DEFAULT_WIDTH;
        let height = DEFAULT_HEIGHT;
        try {
          const size = await getImageSize(thumbnailUrl);
          // Scale up dimensions since thumbnail is smaller
          // This is approximate but good enough for layout
          width = size.width;
          height = size.height;
        } catch {
          // Thumbnail doesn't exist, try full image
          try {
            const size = await getImageSize(url);
            width = size.width;
            height = size.height;
          } catch {
            // Use defaults
          }
        }

        return {
          id: res.id,
          path: res.path,
          url,
          thumbnailUrl,
          width,
          height,
          score: res.score,
          name: res.path.split(/[\\/]/).pop() ?? res.path,
        };
      })
    );

    return images;
  } catch (error) {
    console.error("[Frontend] Error in fetchSimilarImages:", error);
    throw new Error(`Failed to fetch similar images: ${error}`);
  }
}

export async function fetchTieredSimilarImages(imageId: number) {
  try {
    const results: { id: number; path: string; score: number }[] = await invoke(
      "get_tiered_similar_images",
      { imageId }
    );

    // For tiered similar images, construct thumbnail URLs and get dimensions
    const images = await Promise.all(
      results.map(async (res) => {
        const url = convertFileSrc(res.path);
        const thumbnailPath = getThumbnailPath(res.id);
        const thumbnailUrl = convertFileSrc(thumbnailPath);

        // Try to get dimensions from thumbnail (faster) or fall back to full image
        let width = DEFAULT_WIDTH;
        let height = DEFAULT_HEIGHT;
        try {
          const size = await getImageSize(thumbnailUrl);
          width = size.width;
          height = size.height;
        } catch {
          try {
            const size = await getImageSize(url);
            width = size.width;
            height = size.height;
          } catch {
            // Use defaults
          }
        }

        return {
          id: res.id,
          path: res.path,
          url,
          thumbnailUrl,
          width,
          height,
          score: res.score,
          name: res.path.split(/[\\/]/).pop() ?? res.path,
        };
      })
    );

    return images;
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
    const results: {
      id: number;
      path: string;
      score: number;
      thumbnail_path?: string;
      width?: number;
      height?: number;
    }[] = await invoke("semantic_search", { query, topN });

    // Convert backend results to frontend format
    return results.map((res) => ({
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
    }));
  } catch (error) {
    console.error("[Frontend] Error in semanticSearch:", error);
    throw new Error(`Semantic search failed: ${error}`);
  }
}