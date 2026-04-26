import type { ImageItem } from "../types";

/**
 * Pure layout calculation for the Pinterest-style masonry grid.
 *
 * Extracted out of Masonry.tsx so it's unit-testable without DOM
 * mounting. The component reads container width via a ref and feeds
 * it in here; tests provide deterministic widths and item shapes.
 *
 * Algorithm:
 *   1. Determine column count from container width / minItemWidth.
 *      An explicit override beats auto.
 *   2. If a hero item is selected, place it first spanning up to 3
 *      columns from the top-left.
 *   3. For every other item, find the shortest column and place it
 *      there, scaling height to preserve aspect ratio at the column
 *      width.
 *
 * Returns the placed items plus the total grid height.
 */

export interface MasonryItemPlacement {
  itemData: ImageItem;
  x: number;
  y: number;
  width: number;
  /** Rendered height in pixels (aspect-ratio scaled to column width). */
  height: number;
  isSelected: boolean;
}

export interface MasonryLayoutInput {
  items: ImageItem[];
  selectedItem?: ImageItem | null;
  containerWidth: number;
  minItemWidth: number;
  columnGap: number;
  verticalGap: number;
  /** 0 = auto (computed). 1..12 forces. */
  columnCountOverride?: number;
  /** Multiplier on minItemWidth in auto mode. Default 1.0. */
  tileScale?: number;
}

export interface MasonryLayoutOutput {
  placements: MasonryItemPlacement[];
  height: number;
  columnCount: number;
}

export function computeMasonryLayout(
  input: MasonryLayoutInput,
): MasonryLayoutOutput {
  const {
    items,
    selectedItem,
    containerWidth,
    minItemWidth,
    columnGap,
    verticalGap,
    columnCountOverride,
    tileScale = 1.0,
  } = input;

  if (containerWidth <= 0) {
    return { placements: [], height: 0, columnCount: 0 };
  }

  // Column count derivation: explicit override beats auto. Auto uses
  // tile-scaled minimum width. We cap at 12 to prevent absurd values.
  const effectiveMin = minItemWidth * tileScale;
  const autoCount = Math.max(1, Math.floor(containerWidth / effectiveMin));
  const colCount =
    columnCountOverride && columnCountOverride > 0
      ? Math.min(columnCountOverride, 12)
      : autoCount;

  const columnWidth =
    (containerWidth - (colCount - 1) * columnGap) / colCount;
  const placements: MasonryItemPlacement[] = [];
  const colHeights: number[] = new Array(colCount).fill(0);

  // Hero placement: selected item spans up to 3 columns at the top.
  if (selectedItem) {
    const selectedCols = Math.min(colCount, 3);
    const selectedWidth =
      columnWidth * selectedCols + columnGap * (selectedCols - 1);
    const ratio = selectedWidth / selectedItem.width;
    const selectedHeight = selectedItem.height * ratio;

    placements.push({
      itemData: selectedItem,
      x: 0,
      y: 0,
      width: selectedWidth,
      height: selectedHeight,
      isSelected: true,
    });

    for (let i = 0; i < selectedCols; i++) {
      colHeights[i] = selectedHeight + verticalGap;
    }
  }

  // Place remaining items into the shortest column.
  for (const img of items) {
    if (selectedItem && img.id === selectedItem.id) continue;

    let minIndex = 0;
    let minVal = colHeights[0];
    for (let j = 1; j < colCount; j++) {
      if (colHeights[j] < minVal) {
        minIndex = j;
        minVal = colHeights[j];
      }
    }

    const ratio = columnWidth / img.width;
    const itemHeight = img.height * ratio;
    placements.push({
      itemData: img,
      x: minIndex * (columnWidth + columnGap),
      y: colHeights[minIndex],
      width: columnWidth,
      height: itemHeight,
      isSelected: false,
    });
    colHeights[minIndex] += itemHeight + verticalGap;
  }

  const height = colHeights.length > 0 ? Math.max(...colHeights, 0) : 0;
  return { placements, height, columnCount: colCount };
}
