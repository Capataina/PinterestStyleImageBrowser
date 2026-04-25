import { useEffect, useState } from "react";
import { ImageItem, Tag } from "../types";
import { motion, AnimatePresence } from "framer-motion";
import { X, ChevronLeft, ChevronRight } from "lucide-react";
import { TagDropdown } from "./TagDropdown";
import { Badge } from "./ui/badge";
import { RxCrossCircled } from "react-icons/rx";

interface PinterestModalProps {
  item: ImageItem | null;
  onClose: () => void;
  onNavigate?: (direction: "prev" | "next") => void;
  tags?: Tag[];
  onCreateTag: (name: string, color: string) => Promise<Tag>;
  onDeleteTag?: (tagId: number) => void;
  onAssignTag: (imageId: number, tagId: number) => void;
  onRemoveTag: (imageId: number, tagId: number) => void;
}

export function PinterestModal(props: PinterestModalProps) {
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [selectedTags, setSelectedTags] = useState<number[]>([]);

  useEffect(() => {
    if (props.item) {
      setSelectedTags(props.item.tags.map((t) => t.id));
    }
  }, [props.item]);

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      } else if (e.key === "ArrowLeft" && props.onNavigate) {
        props.onNavigate("prev");
      } else if (e.key === "ArrowRight" && props.onNavigate) {
        props.onNavigate("next");
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [props.onClose, props.onNavigate]);

  if (!props.item) return null;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.2 }}
        className="fixed inset-0 z-[100] flex items-center justify-center"
        onClick={props.onClose}
      >
        {/* Backdrop */}
        <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" />

        {/* Modal Container */}
        <motion.div
          initial={{ scale: 0.95, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          exit={{ scale: 0.95, opacity: 0 }}
          transition={{ duration: 0.25, ease: "easeOut" }}
          className="relative z-10 flex max-h-[90vh] max-w-[90vw] overflow-hidden rounded-3xl bg-white shadow-2xl"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Close button */}
          <button
            onClick={props.onClose}
            className="absolute top-4 left-4 z-20 flex h-10 w-10 items-center justify-center rounded-full bg-white/90 shadow-lg backdrop-blur-sm transition-all hover:bg-white hover:scale-105"
          >
            <X className="h-5 w-5 text-gray-700" />
          </button>

          {/* Navigation arrows */}
          {props.onNavigate && (
            <>
              <button
                onClick={() => props.onNavigate?.("prev")}
                className="absolute left-4 top-1/2 z-20 flex h-12 w-12 -translate-y-1/2 items-center justify-center rounded-full bg-white/90 shadow-lg backdrop-blur-sm transition-all hover:bg-white hover:scale-105"
              >
                <ChevronLeft className="h-6 w-6 text-gray-700" />
              </button>
              <button
                onClick={() => props.onNavigate?.("next")}
                className="absolute right-4 top-1/2 z-20 flex h-12 w-12 -translate-y-1/2 items-center justify-center rounded-full bg-white/90 shadow-lg backdrop-blur-sm transition-all hover:bg-white hover:scale-105 md:right-[340px]"
              >
                <ChevronRight className="h-6 w-6 text-gray-700" />
              </button>
            </>
          )}

          {/* Image Section - Uses full resolution URL (not thumbnail) for detailed viewing */}
          <div className="flex max-w-[60vw] items-center justify-center bg-gray-100">
            <img
              src={props.item.url}
              alt={props.item.name}
              className="max-h-[90vh] w-auto object-contain"
              loading="eager"
              decoding="async"
            />
          </div>

          {/* Details Section */}
          <div className="flex w-80 flex-col bg-white p-6">
            {/* Tag dropdown */}
            <div className="mb-6">
              <TagDropdown
                tags={props.tags}
                open={comboboxOpen}
                setOpen={setComboboxOpen}
                selected={selectedTags}
                setSelected={setSelectedTags}
                placeholder="Add Tags"
                instruction="Select tags to add"
                onCreateTag={props.onCreateTag}
                onDeleteTag={props.onDeleteTag}
                imageId={props.item.id}
                onAssignTag={props.onAssignTag}
                onRemoveTag={props.onRemoveTag}
              />
            </div>

            {/* Image name */}
            <h2 className="mb-4 text-lg font-semibold text-gray-900 line-clamp-2">
              {props.item.name}
            </h2>

            {/* Tags */}
            <div className="flex flex-wrap gap-2">
              <AnimatePresence mode="popLayout">
                {props.item.tags.map((tag) => (
                  <motion.div
                    key={tag.id}
                    layout
                    initial={{ opacity: 0, scale: 0.8 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.8 }}
                    transition={{ duration: 0.15 }}
                  >
                    <Badge className="px-3 py-1.5 text-sm">
                      {tag.name}
                      <div
                        className="ml-1.5 cursor-pointer hover:opacity-70"
                        onClick={() => props.onRemoveTag(props.item!.id, tag.id)}
                      >
                        <RxCrossCircled className="h-3.5 w-3.5" />
                      </div>
                    </Badge>
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>

            {/* Spacer */}
            <div className="flex-1" />

            {/* Image info */}
            <div className="mt-6 border-t border-gray-100 pt-4 text-sm text-gray-500">
              <p>{props.item.width} × {props.item.height}px</p>
            </div>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

