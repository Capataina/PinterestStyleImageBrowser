import { memo } from "react";
import { ImageItem } from "../types";
import { motion } from "framer-motion";
import { ZoomIn } from "lucide-react";

interface MasonryItemProps {
  item: ImageItem;
  isSelected?: boolean;
  onClick: (item: ImageItem) => void;
  animationDelay: number;
}

/**
 * MasonryItem displays an image in the grid.
 * Uses thumbnailUrl for faster loading, but full resolution for selected items.
 */
export const MasonryItem = memo(function MasonryItem(props: MasonryItemProps) {
  // Use full resolution for selected image (it's bigger, needs clarity)
  // Use thumbnail for other grid items (much faster loading)
  const displayUrl = props.isSelected
    ? props.item.url
    : (props.item.thumbnailUrl || props.item.url);

  return (
    <motion.div
      layout
      transition={{
        duration: 0.25, // Slightly faster animation
        ease: [0.25, 0.1, 0.25, 1],
        delay: props.animationDelay,
      }}
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97 }}
      whileHover={{ scale: props.isSelected ? 1.01 : 1.02 }}
      onClick={() => props.onClick(props.item)}
      className="cursor-pointer group"
    >
      <div
        className={`relative overflow-hidden rounded-2xl bg-gray-100 transition-all duration-200 ${props.isSelected
          ? "shadow-2xl ring-4 ring-black/20"
          : "shadow-sm group-hover:shadow-xl"
          }`}
      >
        <img
          className="w-full block"
          src={displayUrl}
          alt={props.item.name}
          loading={props.isSelected ? "eager" : "lazy"}
          decoding="async"
        />
        {/* Hover overlay */}
        <div
          className={`absolute inset-0 transition-colors duration-150 ${props.isSelected
            ? "bg-black/0 group-hover:bg-black/20"
            : "bg-black/0 group-hover:bg-black/10"
            }`}
        />
        {/* Click to inspect hint for selected items */}
        {props.isSelected && (
          <div className="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity duration-150">
            <div className="rounded-full bg-white/90 p-4 shadow-lg">
              <ZoomIn className="h-8 w-8 text-gray-700" />
            </div>
          </div>
        )}
        {/* Selected badge */}
        {props.isSelected && (
          <div className="absolute top-3 left-3 rounded-full bg-black/70 px-3 py-1 text-xs font-medium text-white backdrop-blur-sm">
            Click to inspect
          </div>
        )}
      </div>
    </motion.div>
  );
});
