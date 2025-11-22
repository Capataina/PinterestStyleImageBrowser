import { motion } from "framer-motion";
import { ImageItem } from "../types";

interface MasonrySelectedItemProps {
  item: ImageItem;
}

export function MasonrySelectedItem(props: MasonrySelectedItemProps) {
  return (
    <div className="rounded-xl border-2 border-gray-400 px-6 py-5">
      <h1 className="text-ellipsis font-bold overflow-hidden text-xl mb-4">
        {props.item.name}
      </h1>
      <motion.div
        className="rounded-xl overflow-hidden drop-shadow-lg hover:cursor-pointer duration-700 transition-transform ease-out"
        exit={{ opacity: 0 }}
      >
        <img className="w-full" src={props.item.url} />
      </motion.div>
    </div>
  );
}
