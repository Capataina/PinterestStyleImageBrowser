import { useQuery } from "@tanstack/react-query";
import { ImageItem } from "../types";
import { fetchImages } from "../services/images";

export function useImages() {
  return useQuery<ImageItem[]>({
    queryKey: ["images"],
    queryFn: fetchImages,
    enabled: true,
  });
}
