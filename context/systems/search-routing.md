# search-routing

*Maturity: working*

## Scope / Purpose

The frontend's "what should the grid show right now?" decision layer. Lives in `src/pages/[...slug].tsx` (the single catch-all route) and resolves a priority chain — similar > semantic > tag > all — into the `displayImages` set that drives Masonry. Owns the URL ↔ selectedItem reconciliation, the debounced semantic-search trigger, the global keyboard shortcuts, the lazy notes-load on selection, and the typed-error catch-and-format chain.

## Boundaries / Ownership

- **Owns:** the four-tier priority resolution (similar / semantic / tag / all), the URL-slug → selectedItem reconciliation (now using `displayImages` not `images.data` — audit fix), the 300 ms debounce for semantic search, the cmd+, settings shortcut, the cmd+shift+P perf-overlay shortcut (profiling-only), the lazy notes loader, the per-action `recordAction` calls.
- **Does not own:** any IPC (delegates to `src/services/*`), the cache policy (delegates to `src/queries/queryClient`), the actual search SQL (delegates to backend `commands::*`), the Masonry layout (delegates to `Masonry.tsx`), per-tile rendering (delegates to `MasonryItem.tsx`).
- **Public API:** the page component default export `Home`. No exported helpers — the file is a self-contained route.

## Current Implemented Reality

### State held in the page component

```ts
const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
const [isInspecting, setIsInspecting] = useState(false);
const [searchTags, setSearchTags] = useState<Tag[]>([]);
const [searchText, setSearchText] = useState("");
const [settingsOpen, setSettingsOpen] = useState(false);
const [profiling, setProfiling] = useState(false);
const [perfOpen, setPerfOpen] = useState(false);
const [activeNotes, setActiveNotes] = useState<string>("");
const [shuffleSeed, setShuffleSeed] = useState<number>(0);
const { prefs } = useUserPreferences();
```

`pages/[...slug].tsx:28-40, 97, 108`. Plus the URL slug (parsed in a `useEffect` against `useLocation`) is the source of truth for which image is selected.

### Priority chain

```text
1. Similar (highest priority): selectedItem !== null
   ──► useTieredSimilarImages(selectedItem.id) drives displayImages

2. Semantic: searchText non-empty AND not "#"-prefixed AND no selectedItem
   ──► useSemanticSearch(query, prefs.semanticResultCount) drives displayImages

3. Tag filter: searchTags non-empty
   ──► useImages({tagIds: searchTags.map(t => t.id), matchAllTags: ...}) drives displayImages
       (Same hook used as the all-images base; the tag list scopes the SQL)

4. All (default): no selection, no text, no tags
   ──► useImages({tagIds: [], ...}) drives displayImages
```

The hooks don't actually run in priority order — every hook is mounted (or stays disabled via its `enabled` flag) and the page picks the active branch. TanStack Query's `enabled: false` short-circuits the unused branches; the cache for the others stays warm so flipping back is instant.

### URL slug → selection (audit fix)

```ts
useEffect(() => {
    const id = parseInt(location.pathname.replace(/^\//, "").replace(/\/$/, ""), 10);
    if (!isNaN(id)) {
        const found = displayImages.find(img => img.id === id);  // was: images.data?.find
        setSelectedItem(found || null);
    } else {
        setSelectedItem(null);
    }
}, [location.pathname, displayImages]);
```

The audit Known Issues finding `9d04f69` fixed the previous bug where the lookup used `images.data` (the all-images set) instead of `displayImages` (the active priority chain output). Pre-fix, clicking a semantic-search result navigated to its URL but `selectedItem` failed to resolve because the result wasn't in `images.data` — the modal opened with no image, the hero promotion in Masonry didn't fire. Now resolved.

The companion arrow-navigation fix: keyboard navigation now also iterates `displayImages` rather than `images.data`, so prev/next within a semantic-search result set works correctly.

### Debounced semantic search

```ts
const debouncedSearchText = useDebouncedValue(searchText, 300);
const semanticQuery = debouncedSearchText.trim();
const shouldUseSemanticSearch =
    semanticQuery.length > 0 && !semanticQuery.startsWith("#") && !selectedItem;

const semanticSearchResults = useSemanticSearch(
    shouldUseSemanticSearch ? semanticQuery : "",
    prefs.semanticResultCount
);
```

300 ms is short enough to feel responsive, long enough to avoid running a full ONNX inference + cosine sort on every keystroke. The empty string passed when `shouldUseSemanticSearch` is false disables the query (the hook checks for empty input and short-circuits).

### `#` prefix branches to tag autocomplete, not semantic

When the user types `#`, the SearchBar reads it as the start of a tag autocomplete trigger and shows the TagDropdown. The semantic-search trigger explicitly excludes `#`-prefixed text so the user doesn't accidentally fire a 50-result vector search while picking a tag.

### Global keyboard shortcuts

```ts
useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
        const cmdOrCtrl = e.metaKey || e.ctrlKey;
        if (cmdOrCtrl && e.key === ",") {
            setSettingsOpen(s => {
                recordAction(s ? "settings_close" : "settings_open", { via: "shortcut" });
                return !s;
            });
            return;
        }
        if (profiling && cmdOrCtrl && e.shiftKey && (e.key === "P" || e.key === "p")) {
            setPerfOpen(s => !s);
            return;
        }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
}, [profiling]);
```

`pages/[...slug].tsx:57-81`. `cmd+,` toggles the settings drawer (always available). `cmd+shift+P` toggles the perf overlay (only registered when `profiling === true`, gated by the result of `isProfilingEnabled()`).

### Profiling integration

```ts
useEffect(() => {
    isProfilingEnabled().then(on => {
        setProfiling(on);
        if (on) setPerfOpen(true);   // auto-open if launched with --profile
    });
}, []);
```

When `--profile` is set, the overlay auto-opens at mount so the user doesn't have to discover the cmd+shift+P shortcut. Without the flag, every profiling-related code path is dead.

`recordAction` is called at user-action sites (settings open/close, semantic-query started, similar-clicked, tag-mutated). When profiling is off, the IPC call is a no-op on the backend. When on, it appends to the timeline and the on-exit report correlates the next 500 ms of span activity to the action.

### Lazy notes loader

```ts
useEffect(() => {
    if (!selectedItem) {
        setActiveNotes("");
        return;
    }
    let cancelled = false;
    getImageNotes(selectedItem.id)
        .then(n => { if (!cancelled) setActiveNotes(n); })
        .catch(() => { if (!cancelled) setActiveNotes(""); });
    return () => { cancelled = true; };
}, [selectedItem?.id]);
```

Notes are fetched on-demand whenever the user opens an image in the inspector. The cancellation flag prevents a slow IPC from clobbering a fast follow-up (rapid prev/next).

### Typed error catch + format

Every IPC call site uses `formatApiError(e)` (from `services/apiError.ts`) for user-visible toasts. ApiError variants get specific labels ("Tokenizer file missing at ...", "Database error: ..."); legacy `String` errors fall through to `String(error)`; `Error` instances use `e.message`. This is the consumer half of the typed-error wire.

`isMissingModelError(e)` is a helper for the case where the UI wants to trigger a re-download flow instead of just toasting. Today no caller uses it programmatically; the toast message is enough.

### Shuffle seed coordination

```ts
const [shuffleSeed, setShuffleSeed] = useState<number>(0);
```

The `useImages` hook's queryKey includes `shuffleSeed`. When the user closes the inspector modal, the page bumps `shuffleSeed` (`setShuffleSeed(Date.now())`), which causes a refetch with a new key. If the user's `sortMode === "shuffle"`, the frontend applies a deterministic shuffle keyed by the seed, so the order changes on modal close (intentional UX) but stays stable through indexing-progress invalidations (which reuse the same seed).

Default `sortMode` is `"added"` (oldest first by id), so the shuffle seed has no effect for most users — the grid is stable.

## Key Interfaces / Data Flow

### Inputs

| Source | Provides |
|--------|----------|
| URL slug (via `useLocation`) | Selected image id |
| `useImages` hook | All-images set (with optional tag + AND/OR filter) |
| `useTieredSimilarImages` hook | Visual similarity set when an image is selected |
| `useSemanticSearch` hook | Text-driven similarity set when search bar has non-`#` text |
| `useUserPreferences` hook | tagFilterMode, sortMode, semanticResultCount, etc. |
| `useIndexingProgress` hook | Triggers `invalidateQueries(["images"])` on `Phase::Ready` |
| `isProfilingEnabled()` IPC | Resolves once at mount; gates profiling code paths |

### Outputs

| Destination | What |
|-------------|------|
| `<Masonry items={displayImages} selectedItem={selectedItem} ...>` | The set to render |
| `<PinterestModal isOpen={!!selectedItem} ...>` | Modal driven by selection |
| `<SearchBar searchTags={...} onChangeTags={...} />` | Two-way bound search state |
| `<SettingsDrawer open={settingsOpen} onClose={...}>` | Drawer visibility |
| `<PerfOverlay open={perfOpen} onClose={...}>` | Profiling overlay (only when profiling) |
| URL navigation via `navigate("/${id}/")` on tile click | Reflects selection in URL |
| `recordAction(...)` calls at user-action sites | Profiling timeline |

## Implemented Outputs / Artifacts

- A single ~700-line page component that owns the routing logic.
- Two global keyboard shortcuts (settings + perf overlay).
- Lazy notes loading + cancellation.
- Action-breadcrumb integration for the profiling timeline.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Selection lookup race against `displayImages` populating | URL → page navigation while semantic search is mid-flight | The `useEffect` runs once on `[location.pathname, displayImages]`; if `displayImages` is still loading, `selectedItem` stays null. Once the fetch resolves, the effect re-runs and selection lands. Brief blank state. |
| Rapid prev/next inside the modal can race the lazy notes load | Holding arrow key | Old `getImageNotes(prev_id)` resolves after `setActiveNotes("")` for the next image, then writes the wrong notes. The cancellation flag prevents this. |
| `shouldUseSemanticSearch` flips on/off as the user types `#`-then-letter-then-deletes-`#` | Fast typing | Each transition triggers a (possibly debounced) re-query. The debounced text + `enabled: false` for empty queries keep this bounded. |
| `isInspecting` state is set but doesn't appear consumed in the visible head section of the file | Code reading | Verification question for next session: confirm whether `setIsInspecting` is wired anywhere or is dead state. |
| `Profiler` wrapper around Masonry runs the `onRenderProfiler` callback in production builds too | Always | The callback short-circuits internally if profiling is off (no `recordAction` fires); minor render-cost overhead from `Profiler` itself. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Pipeline stats UI**: surface `db::get_pipeline_stats` somewhere visible — top-of-page banner during indexing, status pill secondary line, or settings drawer. Tracked in `plans/pipeline-parallelism-and-stats-ui.md`.
- **Verify `setIsInspecting` is consumed** or remove it.
- **Programmatic re-download trigger** using `isMissingModelError(e)` — a button in the toast that calls a hypothetical `force_reindex` command.
- **Multi-select** for batch tag/note operations (Phase 10 deferred).
- **Drag-and-drop folder add** alongside the dialog plugin path.

## Durable Notes / Discarded Approaches

- **The selection lookup MUST resolve against `displayImages`, not `images.data`.** Fixing this was the audit Known Issues finding `9d04f69`. The bug was subtle: most clicks were on grid tiles whose ids ARE in `images.data`, so the bug only manifested for semantic-search results — exactly the case that's hardest to test and most painful when broken.
- **Default `sortMode` is `"added"`, not `"shuffle"`.** Pre-Phase-9 the backend shuffled on every `get_images_with_thumbnails` call; combined with progressive thumbnail loading this caused the visible "entire app refreshes" UX. The frontend now applies sort modes deterministically via the seed pattern, and stable order is the default.
- **`#` prefix branches to tag autocomplete.** This is opinionated UX — the user might want to search for a literal `#` in image content. The trade-off: tag autocomplete is the much more common operation; literal `#` search is undocumented. A future "use literal `#`" escape (perhaps `\#` or quoted) is possible if the need arises.
- **Profiling overlay auto-opens with `--profile`.** A user who launched with the flag wanted to see the diagnostics; making them discover cmd+shift+P would be hostile.
- **Action breadcrumbs are fire-and-forget.** Awaiting the `record_user_action` IPC would block every user-action handler by ~1 ms. Fire-and-forget keeps the UI responsive at the cost of losing the breadcrumb if the IPC fails (rare).
- **The 300 ms semantic-search debounce was empirical.** Shorter debounces fired too many full-vector-search runs during typing; longer made the search feel laggy. 300 ms is a comfortable sweet spot.

## Obsolete / No Longer Relevant

The pre-audit `images.data` selection lookup is gone. The pre-Phase-9 default `sortMode === "shuffle"` is gone. The pre-typed-error pattern (`catch(e) { showToast(\`Search failed: ${e}\`) }`) is gone — every catch site uses `formatApiError(e)`.
