import { cn } from "@/lib/utils";
import { ImageItem } from "../types";
import { Atropos } from "atropos/react";
import { motion } from "framer-motion";

interface MasonryItemProps {
  item: ImageItem;
  onClick: (item: ImageItem) => void;
  selected: boolean;
  animationDelay: number;
}

export function MasonryItem(props: MasonryItemProps) {
  return (
    <motion.div
      transition={{
        duration: 0.4,
        ease: "easeOut",
        delay: props.animationDelay,
      }}
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0 }}
    >
      <Atropos
        onClick={() => props.onClick(props.item)}
        className={cn("hover:cursor-pointer", props.selected ? "z-50" : "")}
        rotateXMax={5}
        rotateYMax={5}
        activeOffset={10}
        shadowScale={0.9}
      >
        <div className={cn("rounded-xl overflow-hidden")}>
          <img className="w-full" src={props.item.url} />
        </div>
      </Atropos>
    </motion.div>
  );
}
