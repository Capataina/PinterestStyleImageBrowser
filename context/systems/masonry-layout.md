# masonry-layout

*Maturity: working*

## Scope / Purpose

The Pinterest-style grid renderer. Computes column count from container width and a min-item-width prop; performs shortest-column packing for each item; promotes the currently-selected item to a hero card spanning up to 3 columns at the top of the grid. Animates per-item with framer-motion (3D tilt on hover; scale + opacity on enter/exit).

## Boundaries / Ownership

- **Owns:** column-count computation, per-item placement (x, y, width), hero promotion, resize debouncing.
- **Does not own:** image fetching, thumbnail URL construction (delegates to `services/images.ts`), search routing (callers pass the items list).
- **Public API:** `<Masonry items={ImageItem[]} selectedItem={ImageItem|null} columnGap verticalGap minItemWidth onItemClick />`.
- **Component sub-tree:** `Masonry → MasonryAnchor (absolute-positioned wrapper) → MasonryItem (per-image card with tilt)`.

## Current Implemented Reality

### Layout algorithm

```text
on every items/selectedItem change OR resize (debounced 100ms):
    width      = containerRef.current.clientWidth
    colCount   = max(1, floor(width / minItemWidth))
    columnW    = (width - (colCount - 1) * columnGap) / colCount
    colHeights = [0, 0, ..., 0]    # one entry per column

    if selectedItem:
        selectedCols   = min(colCount, 3)
        selectedWidth  = columnW * selectedCols + columnGap * (selectedCols - 1)
        selectedHeight = selectedItem.height * (selectedWidth / selectedItem.width)
        place selectedItem at (0, 0) spanning selectedCols
        colHeights[0..selectedCols] = selectedHeight + verticalGap

    for img in items (skipping selectedItem.id):
        col   = argmin(colHeights)
        ratio = columnW / img.width
        place img at (col * (columnW + columnGap), colHeights[col]) with width = columnW
        colHeights[col] += img.height * ratio + verticalGap

    setHeight(max(colHeights))
```

Source: `Masonry.tsx:24-128`.

The shortest-column heuristic is the standard masonry packing algorithm — guarantees no gaps and a roughly even bottom edge across columns.

### Hero promotion (selected item)

When `selectedItem` is non-null, the algorithm places it first as a wide card spanning up to 3 columns. This is the visual cue that the grid is showing "more like this" — the user's selected image is a large hero at the top of similar results.

Hero width chosen as `min(colCount, 3)` so that on narrow viewports the hero collapses gracefully:
- 1-column viewport: hero spans 1 col (full width, like any other item).
- 2-column viewport: hero spans 2 cols.
- 3+ column viewport: hero spans 3 cols.

### 3D tilt on hover

`MasonryItem.tsx:29-47`:

```text
on mouseMove(e):
    rect    = card.getBoundingClientRect()
    centerX = rect.left + rect.width / 2
    centerY = rect.top  + rect.height / 2
    percentX = (e.clientX - centerX) / (rect.width / 2)   // -1 to 1
    percentY = (e.clientY - centerY) / (rect.height / 2)
    tilt = { rotateX: -percentY * 6, rotateY: percentX * 6 }
```

A spring transition (`stiffness: 300, damping: 20, mass: 0.5`) animates the rotation. On `mouseLeave`, tilt resets to `(0, 0)`.

`perspective: 1000px` is set on the outer wrapper; `transformStyle: preserve-3d` on the rotating div.

### Selection-state styling

The selected item gets:
- `shadow-2xl ring-4 ring-black/20`
- A "Click to inspect" badge in the top-left
- A `ZoomIn` icon overlay that fades in on hover

Hovered (non-selected): `shadow-xl ring-2 ring-black/10`.
Default: `shadow-sm`.

(See `MasonryItem.tsx:94-128`.)

### Performance posture

- `MasonryItem` is wrapped in `React.memo` to avoid re-renders during sibling tilt updates.
- Animation delay is staggered: `Math.min(index * 0.03, 0.5)` — first 17 items get progressive delays, beyond that they animate together.
- Layout debounce on resize is 100ms (`Masonry.tsx:93-97`).
- Thumbnail URL is used unless `isSelected`, in which case the full-resolution URL is loaded — selected items get crispness, the grid stays light.
- `loading="lazy"` on non-selected items, `loading="eager"` on the selected one.

## Key Interfaces / Data Flow

```text
pages/[...slug].tsx
    ──► Masonry items={displayImages} selectedItem={selectedItem} ...

Masonry
    ──► refreshLayout() on items/selectedItem change OR window resize
    ──► <MasonryAnchor> (absolute positioning) per item
        ──► <MasonryItem> with isSelected, animationDelay, onClick
            ──► framer-motion <motion.div> with spring tilt
            ──► <img> with thumbnailUrl or url depending on isSelected
```

## Implemented Outputs / Artifacts

- A grid of absolutely-positioned image cards in a relatively-positioned container.
- Container height is set explicitly to `max(colHeights)` so parent scrolling works.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Re-layout fires on every items/selectedItem change | Search-text changes that produce semantically different result lists | Full re-pack each time. Debounce is only on resize, not on items prop. For 749 items the cost is small; at 10k it would matter. |
| Selection lookup uses `images.data`, not `displayImages` | User clicks a semantic-search result whose id is not in the global image list | `selectedItem` becomes null when navigating into a similar/semantic result whose id is not in `images.data`. Hero card does not render. Documented in the LifeOS Gaps as a UX bug. |
| Default dimensions (800×600) when backend omits w/h | A row in DB without `width`/`height` populated (only happens for very old DBs that pre-date the migration AND where the thumbnail update failed) | Layout treats the image as 800×600, producing wrong aspect ratio. Today this should not happen for any image in `test_images/`. |
| 100ms debounce on resize, 0ms on items | Rapid items-prop changes during tag mutations | Triggers re-layout. The optimistic mutation pattern uses the existing items, so the layout cost is not amplified — but it is observable. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Hero promotion could optionally span more columns at very wide viewports (4 or 5 cols) if the image's aspect ratio supports it.
- Selection lookup should pull from `displayImages` to fix the semantic-search hero gap.
- Virtualised rendering (windowed rendering) would help at 5000+ image scale; today the entire DOM tree is rendered.

## Durable Notes / Discarded Approaches

- **Two earlier components are dead code:** `MasonrySelectedFrame.tsx`, `FullscreenImage.tsx`, `MasonryItemSelected.tsx`. The current `MasonryItem` handles the selected/non-selected distinction inline via the `isSelected` prop. The dead components are still in the repo and add file-tree noise; a single PR could remove them.
- **The hero-spans-3-columns choice is intentional UX, not a layout bug.** A pure top-K masonry shows the selected image at its natural size, mixed in with the results — but that breaks the "this is the focal image" cue. Spanning 3 columns makes the focal point unmissable without modal-blocking the rest of the grid.
- **3D tilt was added in commit `461a7da` (2025-12-17).** The original grid had a flat hover scale; the tilt was added as a visual differentiator. The `perspective: 1000px` is the standard CSS-3D pseudo-distance; the `maxTilt: 6` degrees is deliberately subtle.
- **`React.memo` on `MasonryItem` is load-bearing for the tilt animation.** Without memo, every mouse-move on any tile re-renders all sibling tiles, which destroys the spring animation's smoothness. Keep the memo.

## Obsolete / No Longer Relevant

`MasonryItemSelected.tsx`, `MasonrySelectedFrame.tsx`, `FullscreenImage.tsx` — all superseded by the inline `isSelected` path through `MasonryItem` and the separate `PinterestModal` for the inspect view.
