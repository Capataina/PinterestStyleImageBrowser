import { cn } from "@/lib/utils";
import { ImageItem } from "../types";
import { Atropos } from "atropos/react";

interface MasonryItemProps {
  item: ImageItem;
  onClick: (item: ImageItem) => void;
}

export function MasonryItem(props: MasonryItemProps) {
  return (
    <Atropos
      onClick={() => props.onClick(props.item)}
      className="hover:cursor-pointer"
      rotateXMax={5}
      rotateYMax={5}
      activeOffset={5}
      shadowScale={0.9}
    >
      <div className={cn("rounded-xl overflow-hidden")}>
        <img className="w-full" src={props.item.url} />
      </div>
    </Atropos>
  );
}
