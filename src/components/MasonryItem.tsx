import { memo, useState, useCallback, useRef } from "react";
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
 * Features 3D tilt effect on hover based on mouse position.
 */
export const MasonryItem = memo(function MasonryItem(props: MasonryItemProps) {
  // Use full resolution for selected image (it's bigger, needs clarity)
  // Use thumbnail for other grid items (much faster loading)
  const displayUrl = props.isSelected
    ? props.item.url
    : (props.item.thumbnailUrl || props.item.url);

  const [tilt, setTilt] = useState({ rotateX: 0, rotateY: 0 });
  const [isHovered, setIsHovered] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (!cardRef.current) return;

    const rect = cardRef.current.getBoundingClientRect();
    const centerX = rect.left + rect.width / 2;
    const centerY = rect.top + rect.height / 2;

    // Calculate distance from center as percentage (-1 to 1)
    const percentX = (e.clientX - centerX) / (rect.width / 2);
    const percentY = (e.clientY - centerY) / (rect.height / 2);

    // Max tilt angle in degrees (subtle effect)
    const maxTilt = 6;

    setTilt({
      rotateX: -percentY * maxTilt, // Tilt up when mouse is at top
      rotateY: percentX * maxTilt,  // Tilt right when mouse is at right
    });
  }, []);

  const handleMouseEnter = useCallback(() => {
    setIsHovered(true);
  }, []);

  const handleMouseLeave = useCallback(() => {
    setIsHovered(false);
    setTilt({ rotateX: 0, rotateY: 0 });
  }, []);

  return (
    <motion.div
      ref={cardRef}
      layout
      transition={{
        duration: 0.25,
        ease: [0.25, 0.1, 0.25, 1],
        delay: props.animationDelay,
      }}
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97 }}
      onClick={() => props.onClick(props.item)}
      onMouseMove={handleMouseMove}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      className="cursor-pointer group"
      style={{
        perspective: "1000px",
      }}
    >
      <motion.div
        animate={{
          rotateX: isHovered ? tilt.rotateX : 0,
          rotateY: isHovered ? tilt.rotateY : 0,
          scale: isHovered ? (props.isSelected ? 1.02 : 1.03) : 1,
        }}
        transition={{
          type: "spring",
          stiffness: 300,
          damping: 20,
          mass: 0.5,
        }}
        style={{
          transformStyle: "preserve-3d",
        }}
        className={`relative overflow-hidden rounded-2xl bg-gray-100 transition-shadow duration-200 ${props.isSelected
          ? "shadow-2xl ring-4 ring-black/20"
          : isHovered
            ? "shadow-xl ring-2 ring-black/10"
            : "shadow-sm"
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
            : "bg-black/0 group-hover:bg-black/5"
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
      </motion.div>
    </motion.div>
  );
});
