import { memo, useState, useCallback, useRef } from "react";
import { ImageItem } from "../types";
import { motion } from "framer-motion";

interface MasonryItemProps {
  item: ImageItem;
  isSelected?: boolean;
  isMultiSelected?: boolean;
  onClick: (item: ImageItem) => void;
  onContextMenu?: (item: ImageItem, x: number, y: number) => void;
  animationDelay: number;
  /** Reduce / disable animations per user setting */
  animationLevel?: "subtle" | "standard" | "off";
}

/**
 * MasonryItem displays an image in the grid.
 *
 * Uses thumbnailUrl for faster loading; falls through to the full URL
 * only when the image is the active hero card (selected). 3D tilt on
 * hover responds to mouse position; the magnitude is configurable
 * via the animationLevel prop so the user can tone it down or off.
 *
 * Multi-select highlight uses the warm amber accent token to indicate
 * inclusion in a bulk-tag operation.
 */
export const MasonryItem = memo(function MasonryItem(props: MasonryItemProps) {
  // Use full resolution for selected image (it's bigger, needs clarity).
  // Use thumbnail for grid items (much faster loading + content-visibility friendly).
  const displayUrl = props.isSelected
    ? props.item.url
    : (props.item.thumbnailUrl || props.item.url);

  const [tilt, setTilt] = useState({ rotateX: 0, rotateY: 0 });
  const [isHovered, setIsHovered] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);

  const animationLevel = props.animationLevel ?? "standard";
  // Tilt magnitudes — keep subtle. Standard is 3°, the previous 6° was
  // too much. Subtle drops to 1.5°, off disables tilt entirely.
  const maxTilt =
    animationLevel === "off" ? 0 : animationLevel === "subtle" ? 1.5 : 3;

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!cardRef.current || maxTilt === 0) return;

      const rect = cardRef.current.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;

      const percentX = (e.clientX - centerX) / (rect.width / 2);
      const percentY = (e.clientY - centerY) / (rect.height / 2);

      setTilt({
        rotateX: -percentY * maxTilt,
        rotateY: percentX * maxTilt,
      });
    },
    [maxTilt],
  );

  const handleMouseEnter = useCallback(() => setIsHovered(true), []);
  const handleMouseLeave = useCallback(() => {
    setIsHovered(false);
    setTilt({ rotateX: 0, rotateY: 0 });
  }, []);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (props.onContextMenu) {
        e.preventDefault();
        props.onContextMenu(props.item, e.clientX, e.clientY);
      }
    },
    [props.onContextMenu, props.item],
  );

  return (
    <motion.div
      ref={cardRef}
      layout
      // Use spring physics for layout transitions instead of an ease curve.
      // Stiffer spring = snappier "rearrange" feel as the grid reflows
      // when an image is selected or filtered.
      transition={
        animationLevel === "off"
          ? { duration: 0 }
          : {
              type: "spring",
              stiffness: 350,
              damping: 35,
              mass: 0.8,
              delay: props.animationDelay,
            }
      }
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97 }}
      onClick={() => props.onClick(props.item)}
      onContextMenu={handleContextMenu}
      onMouseMove={handleMouseMove}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      className="masonry-tile cursor-pointer group"
      style={{
        perspective: "1000px",
      }}
    >
      <motion.div
        animate={{
          rotateX: isHovered ? tilt.rotateX : 0,
          rotateY: isHovered ? tilt.rotateY : 0,
          scale: isHovered ? (props.isSelected ? 1.015 : 1.02) : 1,
        }}
        transition={{
          type: "spring",
          stiffness: 400,
          damping: 25,
          mass: 0.5,
        }}
        style={{
          transformStyle: "preserve-3d",
        }}
        className={[
          "relative overflow-hidden rounded-xl bg-card transition-shadow duration-200",
          props.isMultiSelected
            ? "ring-2 ring-primary shadow-lg shadow-primary/20"
            : props.isSelected
              ? "ring-2 ring-primary/60 shadow-2xl"
              : isHovered
                ? "shadow-xl"
                : "shadow-md",
        ].join(" ")}
      >
        <img
          className="w-full block"
          src={displayUrl}
          alt={props.item.name}
          loading={props.isSelected ? "eager" : "lazy"}
          decoding="async"
        />

        {/* Subtle dimming on hover for non-selected tiles. Selected
            and multi-select states already have ring affordances. */}
        {!props.isSelected && !props.isMultiSelected && (
          <div className="absolute inset-0 bg-black/0 group-hover:bg-black/15 transition-colors duration-200 pointer-events-none" />
        )}

        {/* Multi-select indicator — small filled circle in the top-left */}
        {props.isMultiSelected && (
          <div className="absolute top-2 left-2 h-5 w-5 rounded-full bg-primary border-2 border-background shadow-md" />
        )}
      </motion.div>
    </motion.div>
  );
});
