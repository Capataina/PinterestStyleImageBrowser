import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ImageItem } from "../types";
import { MasonryItem } from "./MasonryItem";
import debounce from "lodash/debounce";
import { MasonryAnchor } from "./MasonryAnchor";
import {
  computeMasonryLayout,
  type MasonryItemPlacement,
} from "./masonryPacking";

export type MasonryItemData = MasonryItemPlacement;

interface MasonryProps {
  items?: ImageItem[];
  selectedItem?: ImageItem | null;
  minItemWidth: number;
  columnGap: number;
  verticalGap: number;
  onItemClick: (item: ImageItem) => void;
  /**
   * Override the computed column count. 0 or undefined = auto
   * (computed from container width and minItemWidth, the original
   * behaviour). Otherwise 1..8 forces that count.
   */
  columnCountOverride?: number;
  /** Tile size scale multiplier. Default 1.0. */
  tileScale?: number;
  /** Forwarded to MasonryItem to control 3D tilt magnitude. */
  animationLevel?: "off" | "subtle" | "standard";
}

/**
 * Pixels of overscan above and below the viewport. Items inside this
 * extended band still render so that fast scrolls don't briefly reveal
 * blank tiles before the next paint catches up. One viewport-height of
 * overscan is generous but cheap — at 800px viewport it means we
 * render roughly the visible window plus another window above and
 * below, which is still a tiny fraction of a 2000-image grid.
 */
const VIEWPORT_OVERSCAN_PX = 800;

/**
 * Find the nearest scrolling ancestor by walking up `offsetParent`-ish
 * chain via `parentElement`. We rely on the page chrome enforcing
 * `overflow-y-auto` on a wrapper around the masonry — see
 * `src/pages/[...slug].tsx` where the wrapper has
 * `overflow-y-auto box-border`.
 *
 * Falls back to `document.scrollingElement` (then `document.documentElement`)
 * if no overflowing ancestor is found, which keeps the component safe
 * if it's ever embedded somewhere without the usual wrapper.
 */
function findScrollContainer(el: HTMLElement | null): HTMLElement | null {
  let cur: HTMLElement | null = el?.parentElement ?? null;
  while (cur) {
    const style = window.getComputedStyle(cur);
    const oy = style.overflowY;
    if ((oy === "auto" || oy === "scroll") && cur.scrollHeight > cur.clientHeight) {
      return cur;
    }
    cur = cur.parentElement;
  }
  return (document.scrollingElement as HTMLElement | null) ?? document.documentElement;
}

export default function Masonry(props: MasonryProps) {
  const [items, setItems] = useState<MasonryItemData[]>([]);
  const [height, setHeight] = useState(0);
  // Visible scroll-window: [start, end) in pixels relative to the
  // masonry container's top. Updated on scroll and on layout changes.
  // Initial value is generous so the first paint shows everything in
  // view immediately rather than briefly showing nothing.
  const [viewport, setViewport] = useState<{ top: number; bottom: number }>({
    top: 0,
    bottom: 99999,
  });
  const containerRef = useRef<HTMLDivElement>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  // One-shot mount-time render timing — see comment above the effect
  // that consumes it. Useful as a before/after marker after this
  // virtualisation change (see context/plans/performance-analysis.md
  // — Masonry render p95 was 86ms with max 404ms before this).
  const mountStartedAt = useRef<number>(performance.now());
  const firstPaintLogged = useRef<boolean>(false);

  const refreshLayout = useCallback(() => {
    if (!containerRef.current) return;
    if (!props.items) return;

    const width = containerRef.current.clientWidth;
    const out = computeMasonryLayout({
      items: props.items,
      selectedItem: props.selectedItem ?? null,
      containerWidth: width,
      minItemWidth: props.minItemWidth,
      columnGap: props.columnGap,
      verticalGap: props.verticalGap,
      columnCountOverride: props.columnCountOverride,
      tileScale: props.tileScale,
    });

    setHeight(out.height);
    setItems(out.placements);
  }, [
    props.items,
    props.selectedItem,
    props.verticalGap,
    props.columnGap,
    props.minItemWidth,
    props.columnCountOverride,
    props.tileScale,
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
  }, [props.items, props.selectedItem, refreshLayout]);

  // Scroll-driven viewport tracking. We compute the visible band in
  // container-local coordinates so the cull predicate is just a
  // comparison against `placement.y` and `placement.y + placement.height`.
  // We attach a passive scroll listener so we don't block compositor
  // thread, and we don't rAF-throttle: React 19's automatic batching
  // and the cheap `setViewport` call (only re-renders when the band
  // actually changes) keep this responsive. If profiling later shows
  // the scroll handler itself dominates, swap in a rAF gate.
  const updateViewport = useCallback(() => {
    const container = containerRef.current;
    const scroller = scrollerRef.current;
    if (!container) return;

    let scrollTop: number;
    let viewportH: number;
    if (scroller && scroller !== document.documentElement) {
      scrollTop = scroller.scrollTop;
      viewportH = scroller.clientHeight;
    } else {
      scrollTop = window.scrollY;
      viewportH = window.innerHeight;
    }

    // container's offset within the scroller (top of the masonry
    // relative to the scroll origin). For a non-document scroller we
    // use offsetTop chained up to the scroller; for the document we
    // use getBoundingClientRect against the page.
    let containerOffsetTop = 0;
    if (scroller && scroller !== document.documentElement) {
      let node: HTMLElement | null = container;
      while (node && node !== scroller) {
        containerOffsetTop += node.offsetTop;
        node = node.offsetParent as HTMLElement | null;
      }
    } else {
      const rect = container.getBoundingClientRect();
      containerOffsetTop = rect.top + window.scrollY;
    }

    const localTop = scrollTop - containerOffsetTop;
    const localBottom = localTop + viewportH;

    setViewport((prev) => {
      const next = {
        top: localTop - VIEWPORT_OVERSCAN_PX,
        bottom: localBottom + VIEWPORT_OVERSCAN_PX,
      };
      // Skip the state update if nothing meaningful changed — saves
      // a re-render on idle scroll noise / sub-pixel jitter.
      if (
        Math.abs(prev.top - next.top) < 1 &&
        Math.abs(prev.bottom - next.bottom) < 1
      ) {
        return prev;
      }
      return next;
    });
  }, []);

  useEffect(() => {
    const scroller = findScrollContainer(containerRef.current);
    scrollerRef.current = scroller;
    if (!scroller) return;

    // Bootstrap the viewport once on mount and after layout changes
    // so the first paint culls correctly rather than rendering all
    // 2000 items briefly before the first scroll event.
    updateViewport();

    const target =
      scroller === document.documentElement ? window : scroller;
    target.addEventListener("scroll", updateViewport, { passive: true });
    return () => {
      target.removeEventListener("scroll", updateViewport);
    };
  }, [updateViewport, items.length, height]);

  // One-shot mount-time timing log so before/after virtualisation
  // can be compared on the next perf run. Logs once when items first
  // become non-empty after mount; intentionally NOT once-per-render.
  // Compare against the perf report's Masonry render p95 (86ms before
  // virtualisation; target is well under 50ms).
  useEffect(() => {
    if (firstPaintLogged.current) return;
    if (items.length === 0) return;
    firstPaintLogged.current = true;
    const elapsed = performance.now() - mountStartedAt.current;
    // eslint-disable-next-line no-console
    console.info(
      `[masonry-perf] first non-empty layout: items=${items.length}, ` +
        `elapsed_since_mount_ms=${elapsed.toFixed(1)}`,
    );
  }, [items.length]);

  // Cull placements to the visible band + overscan. This is the
  // structural fix from context/plans/performance-analysis.md (point
  // 4): for a 2000-item grid, only the items intersecting the
  // viewport (typically 30-100 of them) actually mount, dropping
  // Masonry render cost from O(N) to O(visible).
  const visiblePlacements = useMemo(() => {
    if (items.length === 0) return items;
    return items.filter((p) => {
      // Selected hero card always renders so it's never culled out
      // from under the modal/promotion logic, regardless of where
      // the user has scrolled.
      if (p.isSelected) return true;
      const top = p.y;
      const bottom = p.y + p.height;
      return bottom >= viewport.top && top <= viewport.bottom;
    });
  }, [items, viewport.top, viewport.bottom]);

  return (
    <div ref={containerRef} className="w-full relative" style={{ height }}>
      {visiblePlacements.map((item) => (
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
            // Virtualisation note: the previous animationDelay used
            // the item's index in the full list, which produced a
            // pleasing left-to-right cascade on first mount. Under
            // viewport culling, items that were never on-screen
            // never accumulated their delay budget — applying the
            // index-based delay to a tile that mounts mid-scroll
            // would briefly hide it. Use a tiny constant delay
            // instead so freshly-mounted tiles fade in promptly.
            animationDelay={0}
            animationLevel={props.animationLevel}
          />
        </MasonryAnchor>
      ))}
    </div>
  );
}
