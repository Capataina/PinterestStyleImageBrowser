import { ReactNode } from "react";

interface MasonryItemProps {
  x: number;
  y: number;
  width: number;
  children: ReactNode;
}

export function MasonryAnchor(props: MasonryItemProps) {
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
      {props.children}
    </div>
  );
}
