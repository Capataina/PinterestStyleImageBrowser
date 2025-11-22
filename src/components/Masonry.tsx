import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ImageItem } from "../types";
import { MasonryItem } from "./MasonryItem";
import debounce from "lodash/debounce";

interface MasonryProps {
  items: ImageItem[];
  minItemWidth: number;
  columnGap: number;
  verticalGap: number;
}

type MasonryItemData = {
  itemData: ImageItem;
  x: number;
  y: number;
};

export default function Masonry(props: MasonryProps) {
  const [items, setItems] = useState<MasonryItemData[]>([]);
  const colWidthRef = useRef(0);
  const containerRef = useRef<HTMLDivElement>(null);

  const refreshLayout = useCallback(() => {
    if (!containerRef.current) return;

    const width = containerRef.current.clientWidth;
    const colCount = Math.floor(width / props.minItemWidth);
    const columnWidth = (width - (colCount - 1) * props.columnGap) / colCount;
    colWidthRef.current = columnWidth;

    const newItems: MasonryItemData[] = [];
    const colHeights: number[] = [];

    for (let i = 0; i < colCount; i++) {
      colHeights[i] = 0;
    }

    console.log(props.items);

    for (const img of props.items) {
      let minVal = Number.POSITIVE_INFINITY;
      let minIndex = 0;
      for (let j = 0; j < colCount; j++) {
        if (colHeights[j] < minVal) {
          minIndex = j;
          minVal = colHeights[j];
        }
      }

      const ratio = columnWidth / img.width;

      newItems.push({
        itemData: img,
        x: minIndex * (columnWidth + props.columnGap),
        y: colHeights[minIndex],
      });
      colHeights[minIndex] += img.height * ratio + props.verticalGap;
    }

    setItems(newItems);
  }, [props.items, props.verticalGap, props.columnGap, props.minItemWidth]);

  const refreshLayoutDebounced = useMemo(() => {
    return debounce(() => {
      refreshLayout();
    }, 100);
  }, [refreshLayout]);

  useEffect(() => {
    const resizeHandle = () => refreshLayoutDebounced();

    window.addEventListener("resize", resizeHandle);
    return () => window.removeEventListener("resize", resizeHandle);
  }, [refreshLayoutDebounced]);

  useEffect(() => refreshLayout(), [props.items]);

  return (
    <div ref={containerRef} className="w-full h-full relative">
      {items.map((item, idx) => (
        <MasonryItem
          key={idx}
          item={item.itemData}
          x={item.x}
          y={item.y}
          width={colWidthRef.current}
        />
      ))}
    </div>
  );
}
