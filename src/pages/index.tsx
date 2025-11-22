import { useEffect, useState } from "react";
import { ImageItem, ImageData } from "../types";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getImageSize } from "../utils";
import Masonry from "../components/Masonry";

export default function Home() {
  const [images, setImages] = useState<ImageItem[]>([]);

  async function fetchImages() {
    const imagesDB: ImageData[] = await invoke("get_all_images");
    const newImages = await Promise.all(
      imagesDB.map(async (img) => {
        const url = convertFileSrc(img.path);
        const { width, height } = await getImageSize(url);
        return { ...img, url, width, height };
      })
    );

    setImages(newImages);
  }

  useEffect(() => {
    fetchImages();
  }, []);

  return (
    <main className="w-screen h-screen overflow-x-hidden overflow-y-auto">
      <div className="px-10 py-6 w-full h-full relative box-border">
        <Masonry
          items={images}
          columnGap={25}
          verticalGap={25}
          minItemWidth={300}
        />
      </div>
    </main>
  );
}
