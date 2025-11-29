import { useEffect, useState } from "react";
import Masonry, { MasonryItemData } from "../components/Masonry";
import {
  useImages,
  useAssignTagToImage,
  useRemoveTagFromImage,
} from "../queries/useImages";
import { ImageItem, Tag } from "../types";
import { FullscreenImage } from "../components/FullscreenImage";
import { AnimatePresence, motion } from "framer-motion";
import { useLocation, useNavigate } from "react-router";
import { useTags, useCreateTag } from "@/queries/useTags";
import { SearchBar } from "@/components/SearchBar";

export default function Home() {
  const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
  const [focusedItem, setFocusedItem] = useState<MasonryItemData | null>(null);
  const [searchTags, setSearchTags] = useState<Tag[]>([]);
  const [searchText, setSearchText] = useState("");
  const images = useImages({
    tagIds: !selectedItem ? searchTags.map((t) => t.id) : [],
    searchText: searchText,
  });
  const tags = useTags();
  const createTagMutation = useCreateTag();
  const assignTagMutation = useAssignTagToImage();
  const removeTagMutation = useRemoveTagFromImage();

  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    console.log("CHANGED");
    if (images.data) {
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
        <>
          <div className="flex justify-center mb-8">
            <div className="w-full max-w-2xl">
              <AnimatePresence>
                {!selectedItem && (
                  <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 100 }}
                    exit={{ opacity: 0 }}
                  >
                    <SearchBar
                      tags={tags.data}
                      onSearchChange={(selectedTags, text) => {
                        setSearchTags(selectedTags);
                        setSearchText(text);
                      }}
                      placeholder="Search images or type # to filter by tags..."
                      onCreateTag={async (name, color) => {
                        const tag = await createTagMutation.mutateAsync({
                          name,
                          color,
                        });
                        return tag;
                      }}
                    />
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          </div>
          <Masonry
            items={images.data}
            tags={tags.data}
            columnGap={25}
            verticalGap={25}
            minItemWidth={250}
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
              const tag = await createTagMutation.mutateAsync({
                name,
                color,
              });
              return tag;
            }}
            onAssignTag={(imageId, tagId) =>
              assignTagMutation.mutate({ imageId, tagId })
            }
            onRemoveTag={(imageId, tagId) =>
              removeTagMutation.mutate({ imageId, tagId })
            }
          />
        </>
      </div>
    </main>
  );
}
