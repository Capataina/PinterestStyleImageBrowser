import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ImageItem } from "../types";
import { MasonryItem } from "./MasonryItem";
import debounce from "lodash/debounce";
import { MasonryAnchor } from "./MasonryAnchor";
import { MasonrySelectedItem } from "./MasonrySelectedItem";
import { useMeasure } from "../hooks/useMeasure";

interface MasonryProps {
  items: ImageItem[];
  minItemWidth: number;
  columnGap: number;
  verticalGap: number;
  selectedItem?: ImageItem | null;
  onItemClick: (item: ImageItem) => void;
}

type MasonryItemData = {
  itemData: ImageItem;
  x: number;
  y: number;
  width: number;
};

export default function Masonry(props: MasonryProps) {
  const [items, setItems] = useState<MasonryItemData[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);
  const { measure } = useMeasure();

  const refreshLayout = useCallback(async () => {
    if (!containerRef.current) return;

    const width = containerRef.current.clientWidth;
    const colCount = Math.floor(width / props.minItemWidth);
    let selectionCols = Math.min(colCount, 2);

    const columnWidth = (width - (colCount - 1) * props.columnGap) / colCount;

    const newItems: MasonryItemData[] = [];
    const colHeights: number[] = [];

    if (props.selectedItem) {
      const selectedWidth =
        columnWidth * selectionCols + props.columnGap * (selectionCols - 1);
      const { height: selectedHeight, width } = await measure(
        <MasonrySelectedItem item={props.selectedItem} />,
        selectedWidth
      );

      console.log(selectedHeight);
      console.log(width);

      newItems.push({
        x: 0,
        y: 0,
        itemData: props.selectedItem,
        width: selectedWidth,
      });

      for (let i = 0; i < colCount; i++) {
        if (i < selectionCols) {
          colHeights[i] = selectedHeight + props.verticalGap;
        } else {
          colHeights[i] = 0;
        }
      }
    } else {
      for (let i = 0; i < colCount; i++) {
        colHeights[i] = 0;
      }
    }

    for (const img of props.items) {
      if (props.selectedItem && img.url === props.selectedItem.url) continue;

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
        width: columnWidth,
      });
      colHeights[minIndex] += img.height * ratio + props.verticalGap;
    }

    console.log(newItems.length);

    setItems(newItems);
  }, [
    props.items,
    props.verticalGap,
    props.columnGap,
    props.minItemWidth,
    props.selectedItem,
  ]);

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
  }, [props.items, props.selectedItem]);

  return (
    <div ref={containerRef} className="w-full h-full relative">
      {items.map((item) => (
        <MasonryAnchor
          key={item.itemData.url}
          x={item.x}
          y={item.y}
          width={item.width}
        >
          {props.selectedItem?.url == item.itemData.url ? (
            <MasonrySelectedItem item={item.itemData} />
          ) : (
            <MasonryItem item={item.itemData} onClick={props.onItemClick} />
          )}
        </MasonryAnchor>
      ))}
    </div>
  );
}
