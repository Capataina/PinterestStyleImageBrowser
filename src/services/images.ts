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
