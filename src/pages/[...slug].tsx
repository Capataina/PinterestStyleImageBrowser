import { useEffect, useState } from "react";
import Masonry, { MasonryItemData } from "../components/Masonry";
import {
  useImages,
  useAssignTagToImage,
  useRemoveTagFromImage,
} from "../queries/useImages";
import { ImageItem } from "../types";
import { FullscreenImage } from "../components/FullscreenImage";
import { AnimatePresence } from "framer-motion";
import { useLocation, useNavigate } from "react-router";
import { useTags, useCreateTag } from "@/queries/useTags";

export default function Home() {
  const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
  const [focusedItem, setFocusedItem] = useState<MasonryItemData | null>(null);
  const images = useImages();
  const tags = useTags();
  const createTagMutation = useCreateTag();
  const assignTagMutation = useAssignTagToImage();
  const removeTagMutation = useRemoveTagFromImage();

  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    if (images.data) {
      console.log(location.pathname);
      const item = images.data.find(
        (i) =>
          i.id.toString() ===
          location.pathname.substring(1, location.pathname.length - 1)
      );
      if (item) {
        setSelectedItem(item);
      } else {
        setSelectedItem(null);
      }
    }
  }, [location, images.data]);

  const navigatgeBack = () => {
    console.log("bruh");
    navigate(-1);
  };

  return (
    <main className="w-screen h-screen overflow-hidden">
      <AnimatePresence>
        {focusedItem && (
          <FullscreenImage
            setFocusItem={setFocusedItem}
            masonryItem={focusedItem}
          />
        )}
      </AnimatePresence>
      <div className="px-48 py-6 w-full h-full overflow-y-auto box-border">
        {images.data && tags.data && (
          <Masonry
            items={images.data}
            tags={tags.data}
            columnGap={25}
            verticalGap={25}
            minItemWidth={300}
            selectedItem={selectedItem}
            onItemClick={(item) => {
              navigate(`/${item.id}/`);
            }}
            focusedItem={focusedItem}
            onItemFocus={(item) => {
              setFocusedItem(item);
            }}
            navigateBack={navigatgeBack}
            onCreateTag={async (name, color) => {
              const tag = await createTagMutation.mutateAsync({ name, color });
              return tag;
            }}
            onAssignTag={(imageId, tagId) =>
              assignTagMutation.mutate({ imageId, tagId })
            }
            onRemoveTag={(imageId, tagId) =>
              removeTagMutation.mutate({ imageId, tagId })
            }
          />
        )}
      </div>
    </main>
  );
}
