import { useEffect, useState, useMemo } from "react";
import Masonry from "../components/Masonry";
import {
  useImages,
  useAssignTagToImage,
  useRemoveTagFromImage,
} from "../queries/useImages";
import { useTieredSimilarImages } from "../queries/useSimilarImages";
import { ImageItem, Tag } from "../types";
import { AnimatePresence, motion } from "framer-motion";
import { useLocation, useNavigate } from "react-router";
import { useTags, useCreateTag } from "@/queries/useTags";
import { SearchBar } from "@/components/SearchBar";
import { PinterestModal } from "@/components/PinterestModal";

export default function Home() {
  const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
  const [isInspecting, setIsInspecting] = useState(false);
  const [searchTags, setSearchTags] = useState<Tag[]>([]);
  const [searchText, setSearchText] = useState("");
  const images = useImages({
    tagIds: searchTags.map((t) => t.id),
    searchText: searchText,
  });
  const tags = useTags();
  const createTagMutation = useCreateTag();
  const assignTagMutation = useAssignTagToImage();
  const removeTagMutation = useRemoveTagFromImage();
  const tieredSimilarImages = useTieredSimilarImages(selectedItem?.id);

  const location = useLocation();
  const navigate = useNavigate();

  // Find selected item from URL
  useEffect(() => {
    if (images.data) {
      const pathId = location.pathname.replace(/\//g, "");
      const item = images.data.find((i) => i.id.toString() === pathId);
      setSelectedItem(item || null);
      // Reset inspecting state when selection changes
      if (!item) {
        setIsInspecting(false);
      }
    }
  }, [location, images.data]);

  // When an image is selected, show tiered similar images in the grid
  // Otherwise show all images
  const displayImages = useMemo(() => {
    if (selectedItem && tieredSimilarImages.data) {
      // Convert SimilarImageItem to ImageItem format
      return tieredSimilarImages.data.map((sim) => ({
        id: sim.id,
        url: sim.url,
        width: sim.width,
        height: sim.height,
        name: sim.name || "",
        tags: [] as Tag[],
      }));
    }
    return images.data;
  }, [selectedItem, tieredSimilarImages.data, images.data]);

  const handleClose = () => {
    setIsInspecting(false);
    navigate("/");
  };

  const handleCloseInspect = () => {
    setIsInspecting(false);
  };

  const handleNavigate = (direction: "prev" | "next") => {
    if (!images.data || !selectedItem) return;
    const currentIndex = images.data.findIndex((i) => i.id === selectedItem.id);
    if (currentIndex === -1) return;

    let newIndex: number;
    if (direction === "prev") {
      newIndex = currentIndex > 0 ? currentIndex - 1 : images.data.length - 1;
    } else {
      newIndex = currentIndex < images.data.length - 1 ? currentIndex + 1 : 0;
    }
    navigate(`/${images.data[newIndex].id}/`);
  };

  // Handle clicking on an image in the grid
  const handleImageClick = (item: ImageItem) => {
    if (selectedItem && selectedItem.id === item.id) {
      // Clicking on the already-selected image → open inspect modal
      setIsInspecting(true);
    } else {
      // Clicking on a different image → select it
      navigate(`/${item.id}/`);
    }
  };

  return (
    <main className="w-screen h-screen overflow-hidden bg-[#fafafa]">
      {/* Pinterest Modal (inspect mode) - only shows when inspecting a selected image */}
      <AnimatePresence>
        {selectedItem && isInspecting && (
          <PinterestModal
            item={selectedItem}
            onClose={handleCloseInspect}
            onNavigate={handleNavigate}
            tags={tags.data}
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
      </AnimatePresence>

      <div className="px-4 md:px-8 lg:px-16 py-6 w-full h-full overflow-y-auto box-border">
        {/* Search Bar */}
        <div className="flex justify-center mb-8">
          <div className="w-full max-w-2xl">
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
          </div>
        </div>

        {/* Section header when viewing similar images */}
        <AnimatePresence>
          {selectedItem && (
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              className="mb-6 flex items-center justify-between"
            >
              <div>
                <h2 className="text-xl font-semibold text-gray-800">
                  More like this
                </h2>
                <p className="text-sm text-gray-500">
                  {tieredSimilarImages.isFetching
                    ? "Finding similar images..."
                    : `${tieredSimilarImages.data?.length || 0} similar images`}
                </p>
              </div>
              <button
                onClick={handleClose}
                className="rounded-full bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-200"
              >
                ← Back to all
              </button>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Masonry Grid */}
        <Masonry
          items={displayImages}
          selectedItem={selectedItem}
          columnGap={16}
          verticalGap={16}
          minItemWidth={236}
          onItemClick={handleImageClick}
        />
      </div>
    </main>
  );
}
