import { motion } from "framer-motion";
import { ReactNode } from "react";
import { clsx } from "clsx";

interface MasonryItemProps {
  x: number;
  y: number;
  width: number;
  children: ReactNode;
  animationDelay: number;
  selected: boolean;
}

export function MasonryAnchor(props: MasonryItemProps) {
  return (
    <div
      className={clsx(
        "absolute transition-all duration-400 ease-in-out hover:z-50",
        props.selected ? "z-50" : ""
      )}
      style={{
        left: 0,
        top: 0,
        transform: `translate(${props.x}px, ${props.y}px)`,
        width: props.width,
      }}
    >
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
        {props.children}
      </motion.div>
    </div>
  );
}
