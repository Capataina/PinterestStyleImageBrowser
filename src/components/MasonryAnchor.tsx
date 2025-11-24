import { ReactNode } from "react";
import { clsx } from "clsx";

interface MasonryItemProps {
  x: number;
  y: number;
  width: number;
  children: ReactNode;
  visible?: boolean;
  onTop: boolean;
}

export function MasonryAnchor(props: MasonryItemProps) {
  return (
    <div
      className={clsx(
        "absolute transition-transform duration-400 ease-in-out",
        props.visible == false && "invisible",
        props.onTop && "z-50"
      )}
      style={{
        left: 0,
        top: 0,
        transform: `translate(${props.x}px, ${props.y}px)`,
        width: props.width,
      }}
    >
      {props.children}
    </div>
  );
}
