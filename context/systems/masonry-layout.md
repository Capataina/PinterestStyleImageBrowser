# masonry-layout

*Maturity: working*

## Scope / Purpose

The Pinterest-style grid renderer. Computes column count from container width and a min-item-width prop (or honours an explicit override from `useUserPreferences.columnCount`); performs shortest-column packing for each item; promotes the currently-selected item to a hero card spanning up to 3 columns at the top of the grid. Animates per-item with framer-motion (3D tilt on hover; scale + opacity on enter/exit). Tile dimensions come from the backend (`ImageData.width`, `ImageData.height`) — no DOM Image-load round-trip per tile (audit dimensions-to-backend finding).

## Boundaries / Ownership

- **Owns:** column-count computation (auto from container width or honouring an override), shortest-column packing algorithm, hero promotion logic for the selected item, per-tile animation level honouring (`animationLevel` pref).
- **Does not own:** the image data itself (delegates to `search-routing` via the `items` prop), modal opening (delegates to `pages/[...slug].tsx` via `onSelect`), tag editing inside the modal (delegates to `tag-system` via `PinterestModal`).
- **Public API:** `<Masonry items={ImageItem[]} selectedItem={ImageItem | null} columnCountOverride={number} tileScale={number} animationLevel={AnimationLevel} onSelect={(item) => void} />`.

## Current Implemented Reality

### Three components

- `Masonry.tsx` — orchestrator: container measurement, column count, packing, hero promotion
- `MasonryItem.tsx` — per-tile renderer: 3D tilt via framer-motion, honours animationLevel
- `MasonryAnchor.tsx` — absolute-positioned wrapper used to place tiles in computed (x, y) positions

### Column count

```ts
const columnCount = columnCountOverride > 0
    ? columnCountOverride
    : Math.max(1, Math.floor(containerWidth / (minItemWidth * tileScale)));
```

Auto mode (`columnCountOverride === 0`) computes from container width / (minItemWidth * tileScale). The user's `columnCount` preference (0..8) overrides this; 0 means auto. The `tileScale` multiplier (0.6..2.0) lets the user dial tile density without changing the column count.

### Shortest-column packing

```text
columnHeights = [0, 0, ..., 0]  // length = columnCount
for item in items:
    shortest_col = argmin(columnHeights)
    place item at (shortest_col, columnHeights[shortest_col])
    columnHeights[shortest_col] += item_height
```

Each item's rendered height is computed from its aspect ratio (backend-supplied `width / height`) and the column width. Implementation in `src/components/masonryPacking.ts` with unit tests in `masonryPacking.test.ts`.

### Hero promotion

When `selectedItem` is non-null and the corresponding item is in `items`, the selected tile is rendered at the top of the grid spanning up to 3 columns (or fewer if the column count is smaller). The remaining items pack below it via the standard shortest-column algorithm.

This is the visual "this is the image you clicked" cue. Combined with the modal opening, the user sees both the modal AND a promoted hero in the background.

### Backend-supplied dimensions (audit fix)

```ts
interface ImageItem {
    id: number;
    url: string;             // convertFileSrc(thumbnail_path)
    width: number;           // backend-supplied (from images.width column)
    height: number;          // backend-supplied (from images.height column)
    tags: Tag[];
    notes?: string | null;
}
```

Pre-audit, the frontend used a `getImageSize(url)` helper that loaded each thumbnail via `new Image()` to read its natural dimensions. This caused N parallel DOM image loads per masonry render (~50 image loads per grid load) and made the layout briefly wrong while the loads resolved.

The audit fix `fb23bdb` ("dimensions to backend, drop DOM image-loads") moved dimension persistence into the indexing pipeline's thumbnail phase: the original image's width/height are stored in `images.width` and `images.height` and returned by `get_images_with_thumbnails`. Masonry now uses these directly — no DOM round-trip, layout is correct on first render.

For legacy NULL-dimension rows (pre-Phase-9 thumbnail data without dims), the frontend falls back to a default 4:3 aspect ratio.

### Animation level honouring

```ts
// MasonryItem.tsx — based on prefs.animationLevel
switch (animationLevel) {
    case "off":      // no transform on hover, instant transitions
    case "subtle":   // brief opacity transition only
    case "standard": // 3D tilt + spring physics
}
```

Users on slower machines or who find the tilt distracting can dial it down or off. The default is `"standard"`.

### React.Profiler integration

The Masonry tree is wrapped in `<React.Profiler id="masonry" onRender={onRenderProfiler}>` (commit `ee8c5d6`). The callback short-circuits if profiling is off; if on, it calls `recordAction("react.render.masonry", { phase, actualDuration, baseDuration })` so the perf report can correlate React rendering cost with user actions.

## Key Interfaces / Data Flow

### Inputs

| Source | Provides |
|--------|----------|
| `pages/[...slug].tsx` | `items`, `selectedItem`, `columnCountOverride`, `tileScale`, `animationLevel`, `onSelect` |
| Container ref (via `useResizeObserver` or similar) | Container width for auto column count |

### Outputs

| Destination | What |
|-------------|------|
| Container DOM | Absolute-positioned `<MasonryAnchor>` wrappers around each `<MasonryItem>` |
| `onSelect(item)` callback | Fired on tile click; `pages/[...slug].tsx` navigates to `/{id}/` |

## Implemented Outputs / Artifacts

- 3 components: Masonry, MasonryItem, MasonryAnchor
- Pure packing helper in `masonryPacking.ts` with unit tests
- Hero promotion rendering for the selected item
- Backend-sourced dimensions (no DOM Image preload)
- React.Profiler integration for perf attribution

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Hero promotion doesn't show when the selected id is not in `displayImages` | Routing-side gap (prevented by the audit fix; would only happen if `displayImages` is empty) | Modal opens with no hero card promoted in the grid behind it. Already mitigated by the `displayImages` selection lookup fix. |
| Default aspect-ratio fallback for NULL-dimension rows | Legacy thumbnails without stored dims | Tiles render at 4:3 even if the original was portrait. Cosmetic; only affects unmigrated rows. |
| Resize-observer thrash during window resize | Rapid resize | Each pixel of width change recomputes column count + repacks. Throttling could help; not a measured bottleneck. |
| `tileScale` is applied to `minItemWidth` (auto mode) but not to a fixed column override | A user with `columnCount = 4, tileScale = 2.0` | Column count is fixed at 4; tileScale doesn't affect anything visible. Cosmetic; the slider feels broken in fixed-column mode. |
| Framer-motion 3D tilt on hover can be expensive | Many tiles in viewport at once + slower machine | The `animationLevel === "subtle" / "off"` modes exist for this reason. |
| `<React.Profiler>` wrapping always runs `onRenderProfiler` | Production builds | Callback short-circuits if profiling is off; minor render-cost overhead from the Profiler itself. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Apply `tileScale` to fixed-column mode** so the slider has a visible effect even when columns aren't auto.
- **Throttle the resize-observer** to e.g. 50 ms intervals.
- **Virtualisation** for very large grids (10k+ tiles). Today every tile renders even off-screen; framer-motion's enter/exit animations work but the DOM is large.
- **Backfill width/height on legacy rows** in a migration so the default aspect-ratio fallback is rare.

## Durable Notes / Discarded Approaches

- **Backend-supplied dimensions over DOM Image preload.** The audit fix eliminated N DOM Image loads per render; the layout is correct on first paint instead of jumping when each load resolves. The trade-off: width/height columns added to the schema; populated at thumbnail-generation time.
- **Shortest-column packing over CSS Grid `masonry`** because browser support is still gated behind flags in late 2025/early 2026 and the polyfill behaviour is inconsistent. JS-driven packing is fully predictable.
- **Hero promotion via separate render path** (not just CSS span) because the hero card needs distinct sizing logic (always wide enough to feel "promoted") and animation transitions that are hard to express purely in CSS.
- **Animation level as a per-component prop, not a global CSS variable.** Lets MasonryItem decide which animations to skip without each animation needing to read a CSS variable. Slightly more prop drilling; cleaner branching.
- **`<React.Profiler>` wrapper at the Masonry level** because Masonry is the most expensive single component to render — covering it captures most of the React-side cost we'd want to profile.

## Obsolete / No Longer Relevant

The `getImageSize(url)` DOM helper and `waitForAllInnerImages()` patterns are gone — replaced by backend-supplied dimensions. Dead components (`MasonryItemSelected`, `MasonrySelectedFrame`, `FullscreenImage`) flagged in the previous dead-code-inventory have been swept (commit `86df34e`).
