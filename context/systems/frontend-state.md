# frontend-state

*Maturity: comprehensive*

## Scope / Purpose

The shared state layer for the React app. Owns: TanStack Query configuration, the file-based routing setup (`vite-plugin-pages`), the `useUserPreferences` localStorage layer, the `useIndexingProgress` Tauri-event subscription, the `useRoots` mutations, the canonical optimistic-mutation pattern that all tag/root mutations follow, and the settings drawer's split into per-section components.

No global state store exists. `zustand` is declared in `package.json` from earlier planning but never imported — the combination of TanStack Query (server state) + `useUserPreferences` (localStorage-backed prefs) + per-page `useState` (transient UI state) covers every state need in the app.

## Boundaries / Ownership

- **Owns:** `queryClient.ts` (cache policy), routing config (`App.tsx` + `vite.config.ts`), the per-resource query hooks (`src/queries/*`), the `useUserPreferences` hook + localStorage layout, the `useIndexingProgress` event hook, the settings drawer split (`src/components/settings/`).
- **Does not own:** any per-feature query (those live in `queries/use*.ts`), per-page UI state (lives in components via `useState`), the IPC wire format (delegates to `services/*` + `services/apiError.ts`).
- **Public API:** the exported `queryClient`, the `<App />` component composition, `useUserPreferences()`, `useIndexingProgress()`, `useRoots()` + add/remove/setEnabled mutations, the implicit contract that all mutations follow `cancelQueries → snapshot → optimistic → onError rollback → onSuccess invalidate`.

## Current Implemented Reality

### Query client configuration

```ts
new QueryClient({
    defaultOptions: {
        queries: {
            staleTime: Infinity,         // never auto-stale
            gcTime: 10 * 60 * 1000,      // 10-minute cache lifetime
            refetchOnMount: false,
            refetchOnReconnect: false,
            refetchOnWindowFocus: false,
            retry: false,
        },
    },
})
```

`src/queries/queryClient.ts`. Aggressive cache policy because:

- This is a desktop app; there's no "user navigates away and comes back" concept
- IPC calls are local — no network costs to retry
- The backend is the single source of truth; staleness happens deterministically (e.g., on `Phase::Ready` from indexing) and is handled with explicit `invalidateQueries`

### Per-resource hook layout

```
src/queries/
├── queryClient.ts        — staleTime: Infinity, no auto-refetch
├── useImages.ts          — useImages + useAssignTagToImage + useRemoveTagFromImage (optimistic)
├── useTags.ts            — useTags + useCreateTag + useDeleteTag (optimistic)
├── useRoots.ts           — useRoots + useAddRoot + useRemoveRoot + useSetRootEnabled
├── useSimilarImages.ts   — useTieredSimilarImages
└── useSemanticSearch.ts  — 5-min staleTime, 10-min gcTime, debounced from caller
```

`useSemanticSearch` overrides the global staleTime to 5 minutes — semantic queries are deterministic per-input but the user typically doesn't repeat the exact same query within a session. 5-min stale lets a re-issue of the same query within that window hit cache.

### `useImages` query key

```ts
useQuery({
    queryKey: ["images", tagIds, searchText, matchAllTags, sortMode, shuffleSeed],
    queryFn: () => fetchImages(tagIds, searchText, matchAllTags),
    ...
})
```

The cache key includes:
- `tagIds`: filter state
- `searchText`: passed for cache differentiation but ignored by backend SQL
- `matchAllTags`: AND vs OR mode (changes SQL semantics)
- `sortMode`: applied frontend-side, included so toggling re-fetches and re-applies
- `shuffleSeed`: bumped on modal close → triggers refetch with new seed for the deterministic shuffle

### `useUserPreferences` localStorage layer

```ts
export interface UserPreferences {
    theme: ThemeMode;             // "system" | "dark" | "light"
    columnCount: number;          // 0 = auto, else 1..8
    tileMinWidth: number;         // px when columnCount is auto
    sortMode: SortMode;           // "shuffle" | "name" | "added"
    tileScale: number;            // multiplier on minItemWidth in auto mode
    animationLevel: AnimationLevel;  // "off" | "subtle" | "standard"
    similarResultCount: number;   // 5..75
    semanticResultCount: number;  // 10..100
    tagFilterMode: TagFilterMode; // "any" | "all"
}
```

`src/hooks/useUserPreferences.ts:19-41`. Defaults:

```ts
const DEFAULTS: UserPreferences = {
    theme: "system",
    columnCount: 0,           // auto
    tileMinWidth: 236,
    sortMode: "added",        // not shuffle (Phase 9 default change)
    tileScale: 1.0,
    animationLevel: "standard",
    similarResultCount: 35,
    semanticResultCount: 50,
    tagFilterMode: "any",
};
```

Persisted to `localStorage["imageBrowserPrefs"]` as JSON. `theme` is also mirrored to `localStorage["theme"]` so `main.tsx` can apply it before React mounts (avoids the FOUC of wrong-theme flash). Schema is loose — newly-added fields land at their defaults via merge with `DEFAULTS` so older saved JSON deserialises cleanly.

System theme support: when `prefs.theme === "system"`, the hook listens to `window.matchMedia("(prefers-color-scheme: dark)")` so macOS auto-dark-mode flips the app theme along with everything else.

### `useIndexingProgress` event hook

```ts
export function useIndexingProgress(): { progress: IndexingProgress | null }
```

Subscribes to the `"indexing-progress"` Tauri event via `tauri::event::listen`. Stores the most recent payload in React state. The IndexingStatusPill component consumes this directly. The hook is also responsible for calling `invalidateQueries(["images"])` on `phase === "ready"` so the grid re-fetches with the new images visible.

### `useRoots` + mutations

Mirrors the tag pattern but for the roots collection:

```
useRoots()                — read-only listing of roots
useAddRoot()              — optimistic insert + invalidate ["roots"] + ["images"] on success
useRemoveRoot()           — optimistic remove + invalidate ["roots"] + ["images"] on success
useSetRootEnabled()       — optimistic toggle + invalidate ["roots"] + ["images"] on success
```

The `["images"]` invalidation is necessary because root mutations change which images appear in the grid (CASCADE wipe on remove, filter change on enable toggle).

### Settings drawer split (Phase 9 + audit Modularisation finding)

`src/components/settings/` is the split-out drawer. `index.tsx` is the slide-in shell (animation, esc/backdrop dismiss); each section lives in its own file:

```
src/components/settings/
├── index.tsx              — Drawer shell (AnimatePresence + slide animation + esc handler)
├── controls.tsx           — Shared section header + slider/toggle primitives
├── ThemeSection.tsx       — system / dark / light segmented buttons
├── DisplaySection.tsx     — column count slider (0=auto, 1..8), tile scale (0.6..2.0), animation level
├── SearchSection.tsx      — similar / semantic result count sliders, tag filter mode toggle
├── SortSection.tsx        — shuffle / name / added segmented buttons
├── FoldersSection.tsx     — list of configured roots with per-row enable/remove + add-folder button
└── ResetSection.tsx       — "Reset all settings" button → useUserPreferences().resetAll()
```

Each section consumes `useUserPreferences` (and the roots hooks where applicable) directly so they remain testable in isolation without prop-drilling. Pre-Phase-9 + pre-audit, this was a 466-line single `SettingsDrawer.tsx`.

### Optimistic mutation pattern (canonical)

```ts
useMutation({
    mutationFn: (params) => /* IPC call via service */,
    onMutate: async (params) => {
        await queryClient.cancelQueries({ queryKey: [...] });
        const prevData = queryClient.getQueryData([...]);
        queryClient.setQueriesData([...], optimistic update);
        return { prevData };
    },
    onError: (_err, _vars, context) => {
        if (context?.prevData) {
            queryClient.setQueryData([...], context.prevData);
        }
    },
    onSuccess: (data) => { /* swap optimistic placeholder for real data */ },
});
```

Followed by every mutation in the codebase. The reasoning: `staleTime: Infinity` means the only way the UI feels responsive after a mutation is via optimistic updates, and the rollback handles transient IPC failures cleanly.

### `services/*` IPC wrappers

```
src/services/
├── apiError.ts        — ApiError discriminated union + formatApiError + isApiError + isMissingModelError
├── images.ts          — fetchImages, fetchTieredSimilarImages, semanticSearch, pickScanFolder, setScanRoot, getThumbnailPath
├── tags.ts            — fetchTags, createTag (default colour), deleteTag
├── notes.ts           — getImageNotes, setImageNotes
├── roots.ts           — listRoots, addRoot, removeRoot, setRootEnabled
└── perf.ts            — isProfilingEnabled, getPerfSnapshot, recordAction (fire-and-forget),
                          exportPerfSnapshot, perfInvoke (wraps invoke with profiling start/end),
                          onRenderProfiler (React.Profiler callback)
```

Hooks call services; components do not call `invoke` directly. Every catch site uses `formatApiError(e)` for user-visible toasts.

## Key Interfaces / Data Flow

### Inputs

- `QueryClientProvider` wrapping `<App />`
- Tauri IPC events (`indexing-progress`)
- localStorage (`imageBrowserPrefs`, `theme`)
- `prefers-color-scheme` media query

### Outputs

- React Query cache state, consumed by hooks throughout the app
- localStorage writes on every preference change
- `IndexingProgress` state object updated on every event
- `<html class="dark">` toggling on theme change

## Implemented Outputs / Artifacts

- 6 query/mutation hook files in `src/queries/`
- 3 utility hooks in `src/hooks/` (debounce, prefs, indexing-progress)
- 6 services in `src/services/` (incl. perf + apiError)
- 7 settings section components + 1 controls primitive + 1 shell (`src/components/settings/`)
- The implicit "every mutation follows the canonical pattern" contract
- Tests: `useUserPreferences.test.ts` (132 lines), `services.test.ts` (248 lines)

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `staleTime: Infinity` means cache can lag if a backend mutation happens outside a frontend mutation | A second app instance edits the DB (today: not possible — single-process), or a manual DB edit | Stale UI until manual invalidation. Acceptable. |
| `useUserPreferences` writes to localStorage in `update`'s setter | Rapid preference toggling | Each write is sync + small; not a bottleneck. localStorage may be disabled in some WebView modes — falls through to in-memory state silently. |
| `useIndexingProgress` listens once at mount; if mount races the event, the first event may be missed | Very fast indexing (cache-load-only path on second launch) | Pill might not show. Reproducible if cache-load takes <few ms. Cosmetic. |
| Settings sections all consume `useUserPreferences` directly | Re-renders on every preference change | Each section subscribes to the whole prefs object; React batches re-renders. Not a measured bottleneck. |
| `mutation rollback` doesn't refetch | Backend rejection due to staleness | Cache might restore an obsolete entry. `invalidateQueries` on `onSuccess` covers the success path; `onError` doesn't. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Backend-persisted preferences for cross-device sync** if multi-device support is ever added. Today's localStorage-only approach is correct for single-machine.
- **Settings export/import** so a user can share their pref set or move it between machines.
- **Granular cache subscription** if a future profiling pass shows the whole-prefs subscription is causing wasteful re-renders.

## Durable Notes / Discarded Approaches

- **`staleTime: Infinity` + manual invalidation** was a deliberate choice. The alternative — periodic refetch — would re-fire IPC calls without reason. Local IPC is cheap but not free; explicit invalidation is correct.
- **`zustand` was planned in the original memory-bank but never imported.** TanStack Query handled server state (the bulk of state), `useUserPreferences` handles persisted prefs, and per-component `useState` handles transient UI. No need for a global store.
- **Settings split into per-section files** because the single `SettingsDrawer.tsx` had grown to 466 lines and several sections were independently changing. Each section now owns its UI + reads `useUserPreferences` directly. Audit Modularisation finding `f041fc9`.
- **Theme is mirrored to a separate localStorage key** so `main.tsx` can apply it before React mounts. Without this, the app would flash with the wrong theme for a moment on every launch (FOUC).
- **System theme listener is mounted only when `prefs.theme === "system"`** — no point listening when the user has explicitly forced dark or light.
- **`useIndexingProgress` invalidates `["images"]` on Phase::Ready** because that's the user-visible "the catalogue may have changed" signal. Invalidating per-progress event would thrash the cache.
- **Optimistic updates with rollback are mandatory.** Without them, the UI feels stale after every tag/root mutation; with them, the UI feels instant. The rollback covers the rare failure case.
- **`perfInvoke` is opt-in per call site, not an automatic interceptor.** A global interceptor would profile every IPC including ones we don't care about; the explicit wrapper makes profiling intent visible at each call site.

## Obsolete / No Longer Relevant

The pre-Phase-9 single `SettingsDrawer.tsx` is gone (split into `settings/`). The pre-Phase-9 default `sortMode === "shuffle"` is gone (now `"added"`). The pre-typed-error `catch(e) { ... }` patterns that interpolated raw strings are gone — every catch site uses `formatApiError(e)`.
