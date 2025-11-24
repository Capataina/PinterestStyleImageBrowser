import { useRef } from "react";
import { ImageItem } from "../types";
import { MasonryItemData } from "./Masonry";

interface MasonryItemSelectedProps {
  item: ImageItem;
  onClick: (item: MasonryItemData) => void;
}

export function MasonryItemSelected(props: MasonryItemSelectedProps) {
  const imgRef = useRef<HTMLImageElement>(null);

  const onClick = () => {
    const rect = imgRef.current!.getBoundingClientRect();
    props.onClick({
      itemData: props.item,
      x: rect.left,
      y: rect.top,
      width: rect.width,
    });
  };

  return (
    <div
      onClick={onClick}
      className="rounded-xl z-50 overflow-hidden hover:shadow-lg/50 hover:scale-[1.02] hover:cursor-pointer duration-400 transition-all ease-out"
    >
      <img ref={imgRef} id="img" className="w-full" src={props.item.url} />
    </div>
  );
}
