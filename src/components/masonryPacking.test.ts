import { describe, it, expect } from "vitest";
import { computeMasonryLayout } from "./masonryPacking";
import type { ImageItem } from "../types";

/**
 * Pure layout-math tests. No DOM, no React — just shape assertions
 * on the computed placements. These catch regressions in the
 * shortest-column packing logic, the hero-card 3-column promotion,
 * and the column-count override behaviour.
 */

function tile(id: number, w: number, h: number): ImageItem {
  return {
    id,
    url: `mock://${id}`,
    width: w,
    height: h,
    name: `tile-${id}`,
    tags: [],
  };
}

describe("computeMasonryLayout", () => {
  it("returns empty layout when container width is zero", () => {
    const out = computeMasonryLayout({
      items: [tile(1, 100, 100)],
      containerWidth: 0,
      minItemWidth: 200,
      columnGap: 16,
      verticalGap: 16,
    });
    expect(out.placements).toHaveLength(0);
    expect(out.height).toBe(0);
    expect(out.columnCount).toBe(0);
  });

  it("derives column count from container width / min item width", () => {
    // 1000px wide / 200px min = 5 cols (Math.floor)
    const out = computeMasonryLayout({
      items: [],
      containerWidth: 1000,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
    });
    expect(out.columnCount).toBe(5);
  });

  it("respects an explicit column count override", () => {
    const out = computeMasonryLayout({
      items: [],
      containerWidth: 1000,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
      columnCountOverride: 3,
    });
    expect(out.columnCount).toBe(3);
  });

  it("caps the column override at 12 to prevent absurd values", () => {
    const out = computeMasonryLayout({
      items: [],
      containerWidth: 1000,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
      columnCountOverride: 999,
    });
    expect(out.columnCount).toBe(12);
  });

  it("forces at least 1 column even when container is narrower than minItemWidth", () => {
    const out = computeMasonryLayout({
      items: [],
      containerWidth: 50,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
    });
    expect(out.columnCount).toBe(1);
  });

  it("scales auto-derived columns by tileScale", () => {
    // 1000 wide, min 200, scale 2.0 → effective min 400 → 2 cols
    const out = computeMasonryLayout({
      items: [],
      containerWidth: 1000,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
      tileScale: 2.0,
    });
    expect(out.columnCount).toBe(2);
  });

  it("places a single tile in the leftmost column at y=0", () => {
    const out = computeMasonryLayout({
      items: [tile(1, 200, 200)],
      containerWidth: 1000,
      minItemWidth: 200,
      columnGap: 16,
      verticalGap: 16,
      columnCountOverride: 5,
    });
    expect(out.placements).toHaveLength(1);
    const p = out.placements[0];
    expect(p.x).toBe(0);
    expect(p.y).toBe(0);
    expect(p.isSelected).toBe(false);
  });

  it("places multiple items into shortest columns", () => {
    // Equal-aspect square tiles in 3 cols. After 3 placements, every
    // column has one tile of the same height. The 4th tile lands in
    // column 0 (first shortest by argmin).
    const items = [
      tile(1, 100, 100),
      tile(2, 100, 100),
      tile(3, 100, 100),
      tile(4, 100, 100),
    ];
    const out = computeMasonryLayout({
      items,
      containerWidth: 600,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
    });
    expect(out.columnCount).toBe(3);
    expect(out.placements).toHaveLength(4);
    // Item 4 should be in column 0, below item 1.
    const p4 = out.placements[3];
    expect(p4.x).toBe(0);
    expect(p4.y).toBeGreaterThan(0);
  });

  it("promotes the selected item to the top spanning up to 3 columns", () => {
    const selected = tile(1, 1000, 500);
    const items = [selected, tile(2, 100, 100), tile(3, 100, 100)];
    const out = computeMasonryLayout({
      items,
      selectedItem: selected,
      containerWidth: 900,
      minItemWidth: 300,
      columnGap: 0,
      verticalGap: 0,
    });
    // 900 / 300 = 3 cols → hero spans all 3.
    expect(out.columnCount).toBe(3);
    const heroes = out.placements.filter((p) => p.isSelected);
    expect(heroes).toHaveLength(1);
    const hero = heroes[0];
    expect(hero.x).toBe(0);
    expect(hero.y).toBe(0);
    // Hero width should be the full container (3 cols × 300 + 0 gap).
    expect(hero.width).toBe(900);
  });

  it("hero card uses fewer columns when container is narrow", () => {
    const selected = tile(1, 1000, 500);
    const out = computeMasonryLayout({
      items: [selected],
      selectedItem: selected,
      containerWidth: 400,
      minItemWidth: 300,
      columnGap: 0,
      verticalGap: 0,
    });
    // 400 / 300 = 1 col → hero spans 1 col.
    expect(out.columnCount).toBe(1);
    const hero = out.placements[0];
    expect(hero.width).toBe(400);
  });

  it("does not duplicate the selected item when it also appears in items", () => {
    const selected = tile(1, 1000, 500);
    const items = [selected, tile(2, 100, 100)];
    const out = computeMasonryLayout({
      items,
      selectedItem: selected,
      containerWidth: 600,
      minItemWidth: 200,
      columnGap: 0,
      verticalGap: 0,
    });
    // Should have exactly 2 placements: hero + tile 2. The selected
    // item's appearance in `items` must not produce a second tile.
    expect(out.placements).toHaveLength(2);
    expect(out.placements.filter((p) => p.itemData.id === selected.id))
      .toHaveLength(1);
  });

  it("preserves aspect ratio when scaling tiles to column width", () => {
    // 200x100 tile placed in a 100-wide column should have height 50.
    const out = computeMasonryLayout({
      items: [tile(1, 200, 100), tile(2, 100, 100)],
      containerWidth: 200,
      minItemWidth: 100,
      columnGap: 0,
      verticalGap: 0,
    });
    expect(out.columnCount).toBe(2);
    // First tile in col 0 → next item lands in col 1 (since col 0 is
    // taller after the wide-aspect tile). The height field isn't
    // exposed but we can verify the y of tile 2 is less than tile 1's
    // implied bottom.
    const tile1 = out.placements.find((p) => p.itemData.id === 1)!;
    const tile2 = out.placements.find((p) => p.itemData.id === 2)!;
    expect(tile1.x).toBe(0);
    expect(tile2.x).toBe(100);
    expect(tile2.y).toBe(0);
  });

  it("returns a non-zero total height for non-empty layouts", () => {
    const out = computeMasonryLayout({
      items: [tile(1, 100, 100)],
      containerWidth: 200,
      minItemWidth: 100,
      columnGap: 0,
      verticalGap: 0,
    });
    expect(out.height).toBeGreaterThan(0);
  });

  it("emits a per-placement height matching aspect-ratio scaling", () => {
    // 200x100 source tile in a 100-wide column should render at 50px
    // tall. Used by the Masonry viewport-culling pass — see
    // Masonry.tsx and context/plans/performance-analysis.md.
    const out = computeMasonryLayout({
      items: [tile(1, 200, 100)],
      containerWidth: 100,
      minItemWidth: 100,
      columnGap: 0,
      verticalGap: 0,
    });
    expect(out.placements).toHaveLength(1);
    expect(out.placements[0].height).toBeCloseTo(50, 5);
  });

  it("emits a hero placement height scaled to its spanned width", () => {
    const selected = tile(1, 1000, 500);
    const out = computeMasonryLayout({
      items: [selected],
      selectedItem: selected,
      containerWidth: 900,
      minItemWidth: 300,
      columnGap: 0,
      verticalGap: 0,
    });
    // Hero spans all 3 cols → 900px wide → 1000:500 ratio → 450 tall.
    const hero = out.placements.find((p) => p.isSelected)!;
    expect(hero.height).toBeCloseTo(450, 5);
  });
});
