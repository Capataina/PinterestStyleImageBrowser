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
import { IndexingStatusPill } from "@/components/IndexingStatusPill";
import { SettingsDrawer } from "@/components/SettingsDrawer";
import { PerfOverlay } from "@/components/PerfOverlay";
import { isProfilingEnabled, recordAction } from "@/services/perf";
import { useQueryClient } from "@tanstack/react-query";
import { FolderOpen, Settings as SettingsIcon } from "lucide-react";
import { pickScanFolder, setScanRoot } from "@/services/images";
import { useUserPreferences } from "@/hooks/useUserPreferences";
import { getImageNotes, setImageNotes } from "@/services/notes";

export default function Home() {
  const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
  const [isInspecting, setIsInspecting] = useState(false);
  const [searchTags, setSearchTags] = useState<Tag[]>([]);
  const [searchText, setSearchText] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Profiling state — flipped to true once at mount if the binary was
  // launched with `--profile`. Drives three things: whether the perf
  // overlay mounts at all, whether cmd+shift+P does anything, and
  // (later) whether action breadcrumbs get emitted to the backend.
  // Without `--profile`, every profiling-related code path stays cold.
  const [profiling, setProfiling] = useState(false);
  const [perfOpen, setPerfOpen] = useState(false);
  const { prefs } = useUserPreferences();

  // Resolve the profiling flag once at mount. When set, auto-open the
  // overlay so the user doesn't have to discover the cmd+shift+P
  // shortcut — if you started the app with --profile, you wanted to
  // see the diagnostics.
  useEffect(() => {
    isProfilingEnabled().then((on) => {
      setProfiling(on);
      if (on) setPerfOpen(true);
    });
  }, []);

  // Global keyboard shortcuts:
  //   ⌘,        — toggle settings drawer (always available)
  //   ⌘⇧P       — toggle performance overlay (profiling mode only)
  // The shortcuts use ⌘ on macOS and Ctrl elsewhere, both are standard.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmdOrCtrl = e.metaKey || e.ctrlKey;
      if (cmdOrCtrl && e.key === ",") {
        e.preventDefault();
        setSettingsOpen((s) => {
          recordAction(s ? "settings_close" : "settings_open", { via: "shortcut" });
          return !s;
        });
        return;
      }
      if (
        profiling &&
        cmdOrCtrl &&
        e.shiftKey &&
        (e.key === "P" || e.key === "p")
      ) {
        e.preventDefault();
        setPerfOpen((s) => !s);
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [profiling]);

  // Debounce search text for semantic search (300ms delay)
  const debouncedSearchText = useDebouncedValue(searchText, 300);

  // Determine if we should use semantic search:
  // - Has search text that doesn't start with # (tag selector)
  // - No selected item (not viewing similar images)
  const semanticQuery = debouncedSearchText.trim();
  const shouldUseSemanticSearch =
    semanticQuery.length > 0 && !semanticQuery.startsWith("#") && !selectedItem;

  // shuffleSeed bumped only on deliberate refresh actions (currently:
  // closing the inspect modal). Indexing-progress invalidations
  // refetch with the SAME seed so the order stays stable through
  // background updates.
  const [shuffleSeed, setShuffleSeed] = useState<number>(0);

  const images = useImages({
    tagIds: searchTags.map((t) => t.id),
    searchText: searchText,
    matchAllTags: prefs.tagFilterMode === "all",
    sortMode: prefs.sortMode,
    shuffleSeed,
  });

  // Per-image notes (Phase 11). Lazy-loaded when the inspector opens.
  const [activeNotes, setActiveNotes] = useState<string>("");
  useEffect(() => {
    if (!selectedItem) {
      setActiveNotes("");
      return;
    }
    let cancelled = false;
    getImageNotes(selectedItem.id)
      .then((n) => {
        if (!cancelled) setActiveNotes(n);
      })
      .catch(() => {
        // Non-fatal — notes are optional. Treat as empty.
        if (!cancelled) setActiveNotes("");
      });
    return () => {
      cancelled = true;
    };
  }, [selectedItem?.id]);

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
  // queryClient kept around as an explicit import-presence even though
  // unused at the moment — we'll use it again when wiring the perf
  // overlay or other refetch triggers. Suppress the dead-import lint.
  const _queryClient = useQueryClient();
  void _queryClient;

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
    recordAction("image_close", { id: selectedItem?.id });
    setIsInspecting(false);
    // Bump the shuffle seed so the next render produces a fresh order
    // when sortMode is "shuffle". For other sort modes this is a
    // no-op because the cache key includes shuffleSeed but the sort
    // function ignores it.
    if (prefs.sortMode === "shuffle") {
      setShuffleSeed(Date.now() & 0x7fffffff);
    }
    navigate("/");
  };

  const handleCloseInspect = () => {
    recordAction("inspect_close", { id: selectedItem?.id });
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
    const target = images.data[newIndex];
    recordAction("image_navigate", { direction, from: selectedItem.id, to: target.id });
    navigate(`/${target.id}/`);
  };

  // Handle clicking on an image in the grid
  const handleImageClick = (item: ImageItem) => {
    if (selectedItem && selectedItem.id === item.id) {
      // Clicking on the already-selected image → open inspect modal
      recordAction("image_inspect", { id: item.id });
      setIsInspecting(true);
    } else {
      // Clicking on a different image → select it
      recordAction("image_click", { id: item.id });
      navigate(`/${item.id}/`);
    }
  };

  return (
    <main className="w-screen h-screen overflow-hidden bg-background text-foreground">
      {/* Live indexing-progress pill (top-right corner) */}
      <IndexingStatusPill />

      {/* Settings drawer — slides in from the right */}
      <SettingsDrawer
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />

      {/* Performance overlay — toggled with ⌘⇧P */}
      {profiling && (
        <PerfOverlay open={perfOpen} onClose={() => setPerfOpen(false)} />
      )}

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
            notes={activeNotes}
            onSaveNotes={(imageId, notes) => {
              setActiveNotes(notes);
              // Fire-and-forget — non-fatal failure logs but doesn't
              // block the user. The textarea stays in sync via local state.
              setImageNotes(imageId, notes).catch((err) =>
                console.warn("Failed to save notes:", err),
              );
            }}
          />
        )}
      </AnimatePresence>

      <div className="px-4 md:px-8 lg:px-16 py-6 w-full h-full overflow-y-auto box-border">
        {/* Search Bar + folder-picker control */}
        <div className="flex justify-center mb-8">
          <div className="flex w-full max-w-2xl items-center gap-3">
            <div className="flex-1">
              <SearchBar
                tags={tags.data}
                onSearchChange={(selectedTags, text) => {
                  recordAction("search_change", {
                    text,
                    tagIds: selectedTags.map((t) => t.id),
                  });
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
            <button
              type="button"
              title="Choose image folder"
              aria-label="Choose image folder"
              className="flex shrink-0 items-center gap-2 rounded-full bg-secondary text-secondary-foreground px-4 py-3 text-sm font-medium transition-colors hover:bg-accent"
              onClick={async () => {
                try {
                  const folder = await pickScanFolder();
                  if (!folder) return; // user cancelled
                  await setScanRoot(folder);
                } catch (err) {
                  console.error("Folder picker failed:", err);
                  window.alert(
                    `Could not set folder: ${err instanceof Error ? err.message : String(err)}`
                  );
                }
              }}
            >
              <FolderOpen className="h-4 w-4" />
              <span className="hidden md:inline">Choose folder</span>
            </button>

            <button
              type="button"
              title="Settings (⌘,)"
              aria-label="Settings"
              className="flex shrink-0 items-center justify-center rounded-full bg-secondary text-secondary-foreground p-3 transition-colors hover:bg-accent"
              onClick={() => {
                recordAction("settings_open", { via: "button" });
                setSettingsOpen(true);
              }}
            >
              <SettingsIcon className="h-4 w-4" />
            </button>
          </div>
        </div>

        {/* First-launch / empty-state hint */}
        {!selectedItem &&
          !shouldUseSemanticSearch &&
          images.data &&
          images.data.length === 0 && (
            <div className="mb-8 rounded-xl bg-card p-6 text-center shadow-md border border-border">
              <h2 className="mb-2 text-lg font-semibold text-foreground">
                No images yet
              </h2>
              <p className="mb-4 text-sm text-muted-foreground">
                Pick a folder above to start indexing your library. The app
                searches recursively, so you can point it at a parent folder
                and let it sweep through every subfolder.
              </p>
            </div>
          )}

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
                <h2 className="text-xl font-semibold text-foreground">
                  More like this
                </h2>
                <p className="text-sm text-muted-foreground">
                  {tieredSimilarImages.isFetching
                    ? "Finding similar images..."
                    : `${tieredSimilarImages.data?.length || 0} similar images`}
                </p>
              </div>
              <button
                onClick={handleClose}
                className="rounded-full bg-secondary text-secondary-foreground px-4 py-2 text-sm font-medium transition-colors hover:bg-accent"
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
                <h2 className="text-xl font-semibold text-foreground">
                  {isSearchLoading ? (
                    <span className="flex items-center gap-2">
                      <span className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-muted border-t-primary" />
                      Searching for "{semanticQuery}"...
                    </span>
                  ) : (
                    `Results for "${semanticQuery}"`
                  )}
                </h2>
              </div>
              {!isSearchLoading && semanticSearchResults.data && (
                <p className="text-sm text-muted-foreground mt-1">
                  Found {semanticSearchResults.data.length} matching images
                </p>
              )}
              {semanticSearchResults.isError && (
                <p
                  className="text-sm text-destructive mt-1"
                  title={String(semanticSearchResults.error)}
                >
                  Search failed:{" "}
                  {semanticSearchResults.error instanceof Error
                    ? semanticSearchResults.error.message
                    : String(semanticSearchResults.error)}
                </p>
              )}
            </motion.div>
          )}
        </AnimatePresence>

        {/* Masonry Grid — column count and animation level driven by user prefs */}
        <Masonry
          items={displayImages}
          selectedItem={selectedItem}
          columnGap={16}
          verticalGap={16}
          minItemWidth={236}
          columnCountOverride={prefs.columnCount}
          tileScale={prefs.tileScale}
          animationLevel={prefs.animationLevel}
          onItemClick={handleImageClick}
        />
      </div>
    </main>
  );
}
