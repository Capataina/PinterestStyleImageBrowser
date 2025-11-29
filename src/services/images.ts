import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getImageSize } from "../utils";
import { ImageData } from "../types";

export async function fetchImages() {
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
}
