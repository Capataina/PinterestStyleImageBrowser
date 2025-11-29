import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getImageSize } from "../utils";
import { ImageData } from "../types";

export async function fetchImages() {
  try {
    const imagesDB: ImageData[] = await invoke("get_all_images");
    console.log(imagesDB);
    const images = await Promise.all(
      imagesDB.map(async (img) => {
        const url = convertFileSrc(img.path);
        const { width, height } = await getImageSize(url);
        return { ...img, url, width, height, id: img.id.toString() };
      })
    );

    return images;
  } catch (error) {
    throw new Error(`Failed to fetch images: ${error}`);
  }
}

export async function assignTagToImage(
  imageId: string,
  tagId: string
): Promise<void> {
  try {
    await invoke("add_tag_to_image", {
      image_id: parseInt(imageId),
      tag_id: parseInt(tagId),
    });
  } catch (error) {
    throw new Error(`Failed to assign tag: ${error}`);
  }
}
