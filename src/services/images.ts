import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getImageSize } from "../utils";
import { ImageData } from "../types";

export async function fetchImages(
  filterTagIds: number[] = [],
  filterString: string = ""
) {
  try {
    const imagesDB: ImageData[] = await invoke("get_images", {
      filterTagIds,
      filterString,
    });
    console.log(imagesDB);
    const images = await Promise.all(
      imagesDB.map(async (img) => {
        const url = convertFileSrc(img.path);
        const { width, height } = await getImageSize(url);
        return { ...img, url, width, height };
      })
    );

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

export async function fetchSimilarImages(
  imageId: number,
  topN: number = 8
) {
  console.log("[Frontend] fetchSimilarImages called", { imageId, topN });
  
  try {
    console.log("[Frontend] Invoking get_similar_images command...");
    const results: { id: number; path: string; score: number }[] = await invoke(
      "get_similar_images",
      {
        imageId,
        topN,
      }
    );

    console.log("[Frontend] Received results from backend:", {
      count: results.length,
      results: results.map(r => ({ id: r.id, path: r.path.split(/[\\/]/).pop(), score: r.score }))
    });

    console.log("[Frontend] Processing results, converting file paths...");
    const images = await Promise.all(
      results.map(async (res) => {
        const url = convertFileSrc(res.path);
        const { width, height } = await getImageSize(url);
        return {
          id: res.id,
          path: res.path,
          url,
          width,
          height,
          score: res.score,
          name: res.path.split(/[\\/]/).pop() ?? res.path,
        };
      })
    );

    console.log("[Frontend] Final images to return:", {
      count: images.length,
      images: images.map(img => ({ id: img.id, name: img.name, score: img.score }))
    });

    return images;
  } catch (error) {
    console.error("[Frontend] Error in fetchSimilarImages:", error);
    throw new Error(`Failed to fetch similar images: ${error}`);
  }
}

export async function fetchTieredSimilarImages(imageId: number) {
  console.log("[Frontend] fetchTieredSimilarImages called", { imageId });
  
  try {
    const results: { id: number; path: string; score: number }[] = await invoke(
      "get_tiered_similar_images",
      { imageId }
    );

    console.log("[Frontend] Tiered results from backend:", {
      count: results.length,
    });

    const images = await Promise.all(
      results.map(async (res) => {
        const url = convertFileSrc(res.path);
        const { width, height } = await getImageSize(url);
        return {
          id: res.id,
          path: res.path,
          url,
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