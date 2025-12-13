import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ImageItem } from "../types";
import { MasonryItem } from "./MasonryItem";
import debounce from "lodash/debounce";
import { MasonryAnchor } from "./MasonryAnchor";

export type MasonryItemData = {
  itemData: ImageItem;
  x: number;
  y: number;
  width: number;
  isSelected?: boolean;
};

interface MasonryProps {
  items?: ImageItem[];
  selectedItem?: ImageItem | null;
  minItemWidth: number;
  columnGap: number;
  verticalGap: number;
  onItemClick: (item: ImageItem) => void;
}

export default function Masonry(props: MasonryProps) {
  const [items, setItems] = useState<MasonryItemData[]>([]);
  const [height, setHeight] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);

  const refreshLayout = useCallback(() => {
    if (!containerRef.current) return;
    if (!props.items) return;

    const width = containerRef.current.clientWidth;
    const colCount = Math.max(1, Math.floor(width / props.minItemWidth));
    const columnWidth = (width - (colCount - 1) * props.columnGap) / colCount;

    const newItems: MasonryItemData[] = [];
    const colHeights: number[] = new Array(colCount).fill(0);

    // If there's a selected item, place it first spanning 2 columns (or 1 if only 1 column)
    if (props.selectedItem) {
      const selectedCols = Math.min(colCount, 2);
      const selectedWidth = columnWidth * selectedCols + props.columnGap * (selectedCols - 1);
      const ratio = selectedWidth / props.selectedItem.width;
      const selectedHeight = props.selectedItem.height * ratio;

      newItems.push({
        itemData: props.selectedItem,
        x: 0,
        y: 0,
        width: selectedWidth,
        isSelected: true,
      });

      // Update column heights for the columns the selected item spans
      for (let i = 0; i < selectedCols; i++) {
        colHeights[i] = selectedHeight + props.verticalGap;
      }
    }

    // Place the rest of the items
    for (const img of props.items) {
      // Skip the selected item since we already placed it
      if (props.selectedItem && img.id === props.selectedItem.id) continue;

      // Find shortest column
      let minIndex = 0;
      let minVal = colHeights[0];
      for (let j = 1; j < colCount; j++) {
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
        width: columnWidth,
        isSelected: false,
      });
      colHeights[minIndex] += img.height * ratio + props.verticalGap;
    }

    const colMax = Math.max(...colHeights, 0);
    setHeight(colMax);
    setItems(newItems);
  }, [props.items, props.selectedItem, props.verticalGap, props.columnGap, props.minItemWidth]);

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

  useEffect(() => {
    refreshLayout();
  }, [props.items, props.selectedItem, refreshLayout]);

  return (
    <div ref={containerRef} className="w-full relative" style={{ height }}>
      {items.map((item, index) => (
        <MasonryAnchor
          key={`${item.itemData.id}-${item.itemData.url}`}
          x={item.x}
          y={item.y}
          width={item.width}
          onTop={item.isSelected || false}
        >
          <MasonryItem
            item={item.itemData}
            isSelected={item.isSelected}
            onClick={props.onItemClick}
            animationDelay={Math.min(index * 0.03, 0.5)}
          />
        </MasonryAnchor>
      ))}
    </div>
  );
}
