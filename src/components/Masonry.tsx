import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ImageItem, Tag } from "../types";
import { MasonryItem } from "./MasonryItem";
import debounce from "lodash/debounce";
import { MasonryAnchor } from "./MasonryAnchor";
import { MasonrySelectedFrame } from "./MasonrySelectedFrame";
import { useLocate } from "@/hooks/useLocate";
import { MasonryItemSelected } from "./MasonryItemSelected";

export type MasonryItemData = {
  itemData: ImageItem;
  x: number;
  y: number;
  width: number;
};

interface MasonryProps {
  items: ImageItem[];
  tags: Tag[];
  minItemWidth: number;
  columnGap: number;
  verticalGap: number;
  selectedItem?: ImageItem | null;
  onItemClick: (item: ImageItem) => void;
  focusedItem: MasonryItemData | null;
  onItemFocus: (item: MasonryItemData) => void;
  navigateBack: () => void;
  onCreateTag: (name: string, color: string) => Promise<Tag>;
  onAssignTag: (imageId: number, tagId: number) => void;
  onRemoveTag: (imageId: number, tagId: number) => void;
}

export default function Masonry(props: MasonryProps) {
  const [items, setItems] = useState<MasonryItemData[]>([]);
  const [height, setHeight] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const { locate } = useLocate();

  const selectedFrameWidthRef = useRef(0);
  const selectedFrameHeightRef = useRef(0);

  const refreshLayout = useCallback(async () => {
    if (!containerRef.current) return;

    const width = containerRef.current.clientWidth;
    const colCount = Math.floor(width / props.minItemWidth);
    let selectionCols = Math.min(colCount, 2);

    const columnWidth = (width - (colCount - 1) * props.columnGap) / colCount;

    const newItems: MasonryItemData[] = [];
    const colHeights: number[] = [];

    if (props.selectedItem) {
      const selectedFrameWidth =
        columnWidth * selectionCols + props.columnGap * (selectionCols - 1);
      const { height: selectedFrameHeight } = await locate(
        <MasonrySelectedFrame
          item={props.selectedItem}
          navigateBack={props.navigateBack}
          tags={props.tags}
          onCreateTag={props.onCreateTag}
          onAssignTag={props.onAssignTag}
          onRemoveTag={props.onRemoveTag}
        />,
        selectedFrameWidth
      );
      selectedFrameWidthRef.current = selectedFrameWidth;
      selectedFrameHeightRef.current = selectedFrameHeight;

      const {
        x,
        y,
        width: imgWidth,
      } = await locate(
        <MasonrySelectedFrame
          item={props.selectedItem}
          navigateBack={props.navigateBack}
          tags={props.tags}
          onCreateTag={props.onCreateTag}
          onAssignTag={props.onAssignTag}
          onRemoveTag={props.onRemoveTag}
        />,
        selectedFrameWidth,
        "img"
      );

      newItems.push({
        x,
        y,
        itemData: props.selectedItem,
        width: imgWidth,
      });

      for (let i = 0; i < colCount; i++) {
        if (i < selectionCols) {
          colHeights[i] = selectedFrameHeight + props.verticalGap;
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

    let colMax = 0;
    colHeights.forEach((h) => (colMax = Math.max(colMax, h)));

    setHeight(colMax);
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

  const toNextItem = () => {
    const index = props.items.findIndex(
      (i) => i.url == props.selectedItem!.url
    );
  };

  return (
    <div ref={containerRef} className="w-full relative" style={{ height }}>
      <MasonryAnchor
        visible={props.selectedItem != null}
        x={0}
        y={0}
        width={selectedFrameWidthRef.current}
        onTop={true}
      >
        <MasonrySelectedFrame
          height={selectedFrameHeightRef.current}
          item={props.selectedItem}
          navigateBack={props.navigateBack}
          tags={props.tags}
          onCreateTag={props.onCreateTag}
          onAssignTag={props.onAssignTag}
          onRemoveTag={props.onRemoveTag}
        />
      </MasonryAnchor>
      {items.map((item, index) => (
        <MasonryAnchor
          key={item.itemData.url}
          x={item.x}
          y={item.y}
          width={item.width}
          onTop={props.selectedItem?.url == item.itemData.url}
        >
          {props.selectedItem?.url == item.itemData.url ? (
            <MasonryItemSelected
              item={item.itemData}
              onClick={props.onItemFocus}
            />
          ) : (
            <MasonryItem
              item={item.itemData}
              onClick={props.onItemClick}
              animationDelay={index * 0.1 + 0.1}
            />
          )}
        </MasonryAnchor>
      ))}
    </div>
  );
}
