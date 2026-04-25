# search-routing

*Maturity: working*

## Scope / Purpose

The frontend's priority logic that decides which set of images to display in the masonry grid. Combines four input signals (selected image, semantic search query, tag filter, default catalog) into a single `displayImages` array via a useMemo. Lives entirely in the catch-all page route `src/pages/[...slug].tsx`.

## Boundaries / Ownership

- **Owns:** the priority chain, the URL-slug → selected-id parsing, the debounce-and-trigger logic for semantic search.
- **Does not own:** the actual data fetching (delegates to `useImages`, `useTieredSimilarImages`, `useSemanticSearch`), the layout (delegates to `Masonry`).
- **Public API:** none — this is a top-level route component.

## Current Implemented Reality

### Priority chain

```text
displayImages = useMemo:
    if selectedItem AND tieredSimilarImages.data:
        return tieredSimilarImages.data        // [1] Click-to-find-similar
    if shouldUseSemanticSearch AND semanticSearchResults.data:
        return semanticSearchResults.data       // [2] Free-text semantic search
    return images.data                          // [3] Default: tag-filtered or full catalog
```

Source: `pages/[...slug].tsx:71-99`.

There is no [4] tag-filtered branch in the priority chain because tag filtering happens *inside* `images.data` (the `useImages` hook receives the `tagIds` filter and passes it to the backend). When `searchTags` is non-empty, branch [3] still fires but the data is filtered.

### Should-use-semantic-search predicate

```text
debouncedSearchText = useDebouncedValue(searchText, 300ms)
semanticQuery       = debouncedSearchText.trim()
shouldUseSemanticSearch =
    semanticQuery.length > 0
    AND NOT semanticQuery.startsWith("#")    // # is the tag filter syntax
    AND NOT selectedItem                      // can't be in the middle of a similar-images view
```

Source: `pages/[...slug].tsx:26-33`.

### URL → selectedItem

```text
useEffect on [location, images.data]:
    pathId = location.pathname.replace(/\//g, "")
    item   = images.data?.find(i => i.id.toString() === pathId)
    setSelectedItem(item || null)
    if !item: setIsInspecting(false)
```

Source: `pages/[...slug].tsx:57-67`.

The URL is the source of truth for "which image is selected." A click on a tile calls `navigate(`/${item.id}/`)` which updates the URL, which fires the `useEffect`, which updates `selectedItem`, which triggers branch [1] of the priority chain.

The `[...slug]` route name is from `vite-plugin-pages` — it catches every URL path including `/`. The slug is parsed manually (no React Router param wiring) because the path is always either empty (root) or a single id segment.

### State machine

```text
                       ┌──────────────────┐
                       │   default view   │
                       │ (catalog or tag) │
                       └────────┬─────────┘
                                │ user types text
                                ▼
                       ┌──────────────────┐
                       │ semantic results │
                       └────────┬─────────┘
                                │ click on tile → URL change
                                ▼
                       ┌──────────────────┐
                       │ similar results  │
                       │ (selectedItem)   │
                       └────────┬─────────┘
                                │ click "Back to all" or close modal
                                ▼
                          [back to default]
                          (queryClient.invalidateQueries(["images"]))
```

The "Back to all" path explicitly invalidates `["images"]` so the catalog gets a new random shuffle (`pages/[...slug].tsx:114`). This is intentional UX — the grid always feels fresh after clicking back.

### handleNavigate (prev/next in modal)

```text
handleNavigate(direction):
    currentIndex = images.data.findIndex(i => i.id === selectedItem.id)
    if direction === "prev":
        newIndex = currentIndex > 0 ? currentIndex - 1 : images.data.length - 1
    else:
        newIndex = currentIndex < images.data.length - 1 ? currentIndex + 1 : 0
    navigate(`/${images.data[newIndex].id}/`)
```

Source: `pages/[...slug].tsx:122-134`.

This walks **`images.data`**, not `displayImages`. After a semantic search the user can navigate via arrows but they jump through the full catalog, not the result set — a documented UX bug.

## Key Interfaces / Data Flow

```text
[user typing] ──► setSearchText(...)
                        │
                        ▼
               useDebouncedValue (300ms)
                        │
                        ▼
                shouldUseSemanticSearch?
                  yes ──► useSemanticSearch(query, 50)  ──► branch [2]
                  no  ──► (tag filter inside useImages) ──► branch [3]

[user click tile] ──► navigate(`/${id}/`)
                                │
                                ▼
                        useEffect parses URL
                                │
                                ▼
                        setSelectedItem(item)
                                │
                                ▼
                        useTieredSimilarImages(id) ──► branch [1]
```

## Implemented Outputs / Artifacts

- A single `displayImages` array consumed by `<Masonry items={displayImages} ... />`.
- A `selectedItem` ref that drives Masonry's hero promotion and `<PinterestModal>` content.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| URL → selectedItem lookup uses `images.data`, not `displayImages` | User navigates to a semantic-search result whose id is not in the catalog query | `selectedItem` becomes `null` for that id; hero card does not promote; arrow navigation falls through. |
| Arrow nav uses `images.data`, not `displayImages` | User in semantic-search results presses ←/→ | Navigates through the global catalog instead of the result set. Surprising UX. |
| `setSearchText` triggers a `useImages` cache miss | Every keystroke produces a new cache key `["images", tagIds, searchText]` | Wasted memory; the backend ignores `searchText` so the data is identical to the previous keystroke's. |
| Generic semantic-search error message | Any failure in the IPC chain | The user sees "Search failed. Make sure the text model is available." regardless of cause. |
| Modal-close invalidates `["images"]` exact-key | Closing the inspect modal | Triggers a refetch which produces a new random shuffle. Intended, but worth documenting because it surprises if an engineer expects state to be preserved across modal toggles. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Use `displayImages` for `selectedItem` lookup and arrow navigation so semantic-search results work consistently.
- Drop `searchText` from the `useImages` query key (the backend ignores it).
- Add a fourth priority branch for tag-only filtering if AND/OR semantics ever ship — currently the OR semantics live silently inside branch [3].
- Surface real error strings (today's error UI is generic).

## Durable Notes / Discarded Approaches

- **The single-input search bar is intentional UX.** The `SearchBar` component takes one input that is both a tag filter (via `#tag` prefix) and a semantic search (otherwise). The shouldUseSemanticSearch predicate is the disambiguation layer. An earlier alternative (separate text and tag inputs) would have been more discoverable but more cluttered. The current approach hinges on the placeholder text teaching the `#` syntax.
- **The 300ms debounce on the semantic-search trigger is deliberate.** Without it, every keystroke would fire an IPC call and an ONNX session.run. The 300ms is human-tuned: long enough that fast typing doesn't fire mid-typing, short enough that pause-to-read doesn't feel laggy.
- **Random shuffle on modal close** is the second-most-product-thoughtful decision after the tiered cosine sampler. Without it, the grid order is stable across sessions and starts to feel like a list. Random ordering keeps re-visits feeling like discovery. The implementation is `queryClient.invalidateQueries({ queryKey: ["images"] })` paired with the backend's `rand::rng()` shuffle at `db.rs:496-499`.
- **The catch-all `[...slug]` route name is a vite-plugin-pages convention.** A future React Router migration could replace it with `:imageId?` or similar — but today the parsing is dead-simple (`replace(/\//g, "")`) and works fine.

## Obsolete / No Longer Relevant

None.
