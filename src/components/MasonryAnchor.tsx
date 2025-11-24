import { ReactNode } from "react";
import { clsx } from "clsx";

interface MasonryItemProps {
  x: number;
  y: number;
  width: number;
  children: ReactNode;
  visible?: boolean;
}

export function MasonryAnchor(props: MasonryItemProps) {
  return (
    <div
      className={clsx(
        "absolute transition-all duration-400 ease-in-out hover:z-50",
        props.visible == false ? "invisible" : ""
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
