import { create } from "zustand";
import { ImageItem } from "../types";

type ImageStore = {
  selectedImage: ImageItem | null;
};

export const useImageStore = create<ImageStore>((set) => ({
  selectedImage: null,

  setSelectedImage(img: ImageItem | null) {
    set(() => ({ selectedImage: img }));
  },
}));
