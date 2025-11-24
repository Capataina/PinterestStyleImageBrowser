import { ImageItem } from "../types";

interface MasonryItemSelectedProps {
  item: ImageItem;
  onClick: (item: ImageItem) => void;
}

export function MasonryItemSelected(props: MasonryItemSelectedProps) {
  return (
    <div
      onClick={() => props.onClick(props.item)}
      className="rounded-xl z-50 overflow-hidden hover:shadow-lg/50 hover:scale-[1.02] hover:cursor-pointer duration-400 transition-all ease-out"
    >
      <img id="img" className="w-full" src={props.item.url} />
    </div>
  );
}
