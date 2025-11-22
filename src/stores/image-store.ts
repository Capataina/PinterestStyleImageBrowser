import { create } from "zustand";
import { ImageItem } from "../types";

type ImageStoreType = {
  images: ImageItem[];
};

const useImageStore = create<ImageStoreType>((set) => ({
  images: [],
}));
