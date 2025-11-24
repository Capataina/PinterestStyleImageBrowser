import { useLayoutEffect, useRef, useState } from "react";
import { MasonryItemData } from "./Masonry";
import { motion } from "framer-motion";

interface FullscreenImageProps {
  masonryItem: MasonryItemData;
  setFocusItem: (item: MasonryItemData | null) => void;
}

export function FullscreenImage(props: FullscreenImageProps) {
  const imgRef = useRef<HTMLImageElement>(null);
  const initialData = useRef({
    x: props.masonryItem.x,
    y: props.masonryItem.y,
    width: props.masonryItem.width,
  });
  const [data, setData] = useState({
    x: props.masonryItem.x,
    y: props.masonryItem.y,
    width: props.masonryItem.width,
  });

  useLayoutEffect(() => {
    requestAnimationFrame(() => {
      const rect = imgRef.current?.getBoundingClientRect()!;

      let newWidth = 0;
      let newHeight = 0;
      if (rect.width > rect.height) {
        newWidth = window.innerWidth * 0.6;
        const ratio = newWidth / rect.width;
        newHeight = rect.height * ratio;
      } else {
        newHeight = window.innerHeight * 0.8;
        const ratio = newHeight / rect.height;
        newWidth = rect.width * ratio;
      }

      setData({
        x: window.innerWidth / 2 - newWidth / 2,
        y: window.innerHeight / 2 - newHeight / 2,
        width: newWidth,
      });
    });
  }, []);

  const leaveFocus = () => {
    setData(initialData.current);
    props.setFocusItem(null);

    // setTimeout(() => {
    // }, 400);
  };

  return (
    <motion.div
      className="absolute top-0 left-0 right-0 bottom-0"
      style={{ zIndex: 100 }}
      initial={{
        backdropFilter: "blur(0px)",
        backgroundColor: "rgba(0, 0, 0, 0)",
      }}
      animate={{
        backdropFilter: "blur(12px)",
        backgroundColor: "rgba(0, 0, 0, 0.8)",
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
        className="absolute transition-all opacity-100 duration-300"
        src={props.masonryItem.itemData.url}
        style={{
          transform: `translate(${data.x}px, ${data.y}px)`,
          width: `${data.width}px`,
        }}
      />
    </motion.div>
  );
}
