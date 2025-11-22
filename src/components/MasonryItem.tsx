import { ImageItem } from "../types";
import { motion } from "framer-motion";

interface MasonryItemProps {
  item: ImageItem;
  x: number;
  y: number;
  width: number;
}

export function MasonryItem(props: MasonryItemProps) {
  return (
    <div
      className="absolute transition-all ease-in-out hover:z-50"
      style={{
        left: 0,
        top: 0,
        transform: `translate(${props.x}px, ${props.y}px)`,
        width: props.width,
      }}
    >
      <motion.div
        className="hover:scale-105 rounded-xl overflow-hidden hover:cursor-pointer hover:drop-shadow-xl duration-300 transition-transform ease-out"
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0 }}
      >
        <img src={props.item.url} />
      </motion.div>
    </div>
  );
}
