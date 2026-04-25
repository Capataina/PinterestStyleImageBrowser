import { useEffect, useState, useMemo } from "react";
import Masonry from "../components/Masonry";
import {
  useImages,
  useAssignTagToImage,
  useRemoveTagFromImage,
} from "../queries/useImages";
import { useTieredSimilarImages } from "../queries/useSimilarImages";
import { useSemanticSearch } from "../queries/useSemanticSearch";
import { useDebouncedValue } from "../hooks/useDebouncedValue";
import { ImageItem, Tag } from "../types";
import { AnimatePresence, motion } from "framer-motion";
import { useLocation, useNavigate } from "react-router";
import { useTags, useCreateTag, useDeleteTag } from "@/queries/useTags";
import { SearchBar } from "@/components/SearchBar";
import { PinterestModal } from "@/components/PinterestModal";
import { useQueryClient } from "@tanstack/react-query";

export default function Home() {
  const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
  const [isInspecting, setIsInspecting] = useState(false);
  const [searchTags, setSearchTags] = useState<Tag[]>([]);
  const [searchText, setSearchText] = useState("");

  // Debounce search text for semantic search (300ms delay)
  const debouncedSearchText = useDebouncedValue(searchText, 300);

  // Determine if we should use semantic search:
  // - Has search text that doesn't start with # (tag selector)
  // - No selected item (not viewing similar images)
  const semanticQuery = debouncedSearchText.trim();
  const shouldUseSemanticSearch =
    semanticQuery.length > 0 && !semanticQuery.startsWith("#") && !selectedItem;

  const images = useImages({
    tagIds: searchTags.map((t) => t.id),
    searchText: searchText,
  });

  // Semantic search query (only runs when shouldUseSemanticSearch is true)
  const semanticSearchResults = useSemanticSearch(
    shouldUseSemanticSearch ? semanticQuery : "",
    50
  );

  const tags = useTags();
  const createTagMutation = useCreateTag();
  const deleteTagMutation = useDeleteTag();
  const assignTagMutation = useAssignTagToImage();
  const removeTagMutation = useRemoveTagFromImage();
  const tieredSimilarImages = useTieredSimilarImages(selectedItem?.id);

  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();

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

  // Determine which images to display:
  // Priority: 1) Similar images (when image selected) > 2) Semantic search > 3) All images
  const displayImages = useMemo(() => {
    // 1. If an image is selected, show tiered similar images
    if (selectedItem && tieredSimilarImages.data) {
      return tieredSimilarImages.data.map((sim) => ({
        id: sim.id,
        url: sim.url,
        thumbnailUrl: sim.thumbnailUrl,
        width: sim.width,
        height: sim.height,
        name: sim.name || "",
        tags: [] as Tag[],
      }));
    }

    // 2. If semantic search is active and has results, show those
    if (shouldUseSemanticSearch && semanticSearchResults.data) {
      return semanticSearchResults.data.map((sim) => ({
        id: sim.id,
        url: sim.url,
        thumbnailUrl: sim.thumbnailUrl,
        width: sim.width,
        height: sim.height,
        name: sim.name || "",
        tags: [] as Tag[],
      }));
    }

    // 3. Default: show all images (with optional tag filter)
    return images.data;
  }, [
    selectedItem,
    tieredSimilarImages.data,
    shouldUseSemanticSearch,
    semanticSearchResults.data,
    images.data,
  ]);

  // Determine if we're in a loading state
  const isSearchLoading = shouldUseSemanticSearch && semanticSearchResults.isFetching;

  const handleClose = () => {
    setIsInspecting(false);
    // Invalidate images query to force refetch with new random order
    queryClient.invalidateQueries({ queryKey: ["images"] });
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
            onDeleteTag={(tagId) => deleteTagMutation.mutate(tagId)}
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

        {/* Semantic search status */}
        <AnimatePresence>
          {shouldUseSemanticSearch && !selectedItem && (
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              className="mb-6"
            >
              <div className="flex items-center gap-2">
                <h2 className="text-xl font-semibold text-gray-800">
                  {isSearchLoading ? (
                    <span className="flex items-center gap-2">
                      <span className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-gray-300 border-t-gray-600" />
                      Searching for "{semanticQuery}"...
                    </span>
                  ) : (
                    `Results for "${semanticQuery}"`
                  )}
                </h2>
              </div>
              {!isSearchLoading && semanticSearchResults.data && (
                <p className="text-sm text-gray-500 mt-1">
                  Found {semanticSearchResults.data.length} matching images
                </p>
              )}
              {semanticSearchResults.isError && (
                <p className="text-sm text-red-500 mt-1">
                  Search failed. Make sure the text model is available.
                </p>
              )}
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
