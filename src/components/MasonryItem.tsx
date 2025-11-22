import { ImageItem } from "../types";
import { motion } from "framer-motion";

interface MasonryItemProps {
  item: ImageItem;
  onClick: (item: ImageItem) => void;
}

export function MasonryItem(props: MasonryItemProps) {
  return (
    <motion.div
      onClick={() => props.onClick(props.item)}
      className="hover:scale-105 rounded-xl overflow-hidden hover:cursor-pointer hover:drop-shadow-xl duration-300 transition-transform ease-out"
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
    >
      <img className="w-full" src={props.item.url} />
    </motion.div>
  );
}
