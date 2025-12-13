import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ImageItem } from "../types";
import { motion } from "framer-motion";

// Legacy type for backwards compatibility with masonry
export interface MasonryItemData {
  itemData: ImageItem;
  x: number;
  y: number;
  width: number;
}

interface FullscreenImageProps {
  // Support both masonry item and direct image with rect
  masonryItem?: MasonryItemData;
  image?: ImageItem;
  initialRect?: DOMRect;
  onClose: () => void;
}

export function FullscreenImage(props: FullscreenImageProps) {
  const imgRef = useRef<HTMLImageElement>(null);
  
  // Get initial values from either masonryItem or initialRect
  const imageData = props.masonryItem?.itemData || props.image;
  const initialX = props.masonryItem?.x ?? props.initialRect?.left ?? 0;
  const initialY = props.masonryItem?.y ?? props.initialRect?.top ?? 0;
  const initialWidth = props.masonryItem?.width ?? props.initialRect?.width ?? 300;
  
  const initialData = useMemo(
    () => ({
      x: initialX,
      y: initialY,
      width: initialWidth,
    }),
    [initialX, initialY, initialWidth]
  );
  
  const [data, setData] = useState({
    x: initialX,
    y: initialY,
    width: initialWidth,
  });

  const recalculatePosition = () => {
    if (!imageData) return;
    
    const aspectRatio = imageData.width / imageData.height;
    
    let newWidth: number;
    let newHeight: number;
    
    // Fit image to 90% of viewport while maintaining aspect ratio
    const maxWidth = window.innerWidth * 0.9;
    const maxHeight = window.innerHeight * 0.9;
    
    if (maxWidth / aspectRatio <= maxHeight) {
      // Width constrained
      newWidth = maxWidth;
      newHeight = maxWidth / aspectRatio;
    } else {
      // Height constrained
      newHeight = maxHeight;
      newWidth = maxHeight * aspectRatio;
    }

    setData({
      x: window.innerWidth / 2 - newWidth / 2,
      y: window.innerHeight / 2 - newHeight / 2,
      width: newWidth,
    });
  };

  useLayoutEffect(() => {
    requestAnimationFrame(recalculatePosition);
  }, []);

  useEffect(() => {
    window.addEventListener("resize", recalculatePosition);
    return () => {
      window.removeEventListener("resize", recalculatePosition);
    };
  }, []);

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [props.onClose]);

  const leaveFocus = () => {
    setData(initialData);
    props.onClose();
  };

  if (!imageData) return null;

  return (
    <motion.div
      className="fixed top-0 left-0 right-0 bottom-0"
      style={{ zIndex: 200 }}
      initial={{
        backdropFilter: "blur(0px)",
        backgroundColor: "rgba(0, 0, 0, 0)",
      }}
      animate={{
        backdropFilter: "blur(16px)",
        backgroundColor: "rgba(0, 0, 0, 0.9)",
      }}
      exit={{
        backdropFilter: "blur(0px)",
        backgroundColor: "rgba(0, 0, 0, 0)",
      }}
      transition={{ duration: 0.3, ease: "easeOut" }}
      onClick={leaveFocus}
    >
      <img
        ref={imgRef}
        className="absolute transition-all opacity-100 duration-300 rounded-lg shadow-2xl"
        src={imageData.url}
        alt={imageData.name}
        style={{
          transform: `translate(${data.x}px, ${data.y}px)`,
          width: `${data.width}px`,
        }}
      />
    </motion.div>
  );
}
