# frontend-state

*Maturity: working*

## Scope / Purpose

The shared state layer for the React app. Owns: TanStack Query configuration, the file-based routing setup (`vite-plugin-pages`), and the standard optimistic-mutation pattern that all tag mutations follow. No global store (zustand is declared in `package.json` but never imported).

## Boundaries / Ownership

- **Owns:** `queryClient.ts` (cache policy), routing config (`App.tsx` + `vite.config.ts`), the optimistic mutation pattern.
- **Does not own:** any per-feature query (those live in `queries/use*.ts`), per-page UI state (lives in components via `useState`).
- **Public API:** the exported `queryClient`, the `<App />` component composition, and the implicit contract that all mutations follow `cancelQueries → snapshot → optimistic → onError rollback`.

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
});
```

Source: `src/queries/queryClient.ts`. This is an unusually aggressive cache-and-never-refetch policy. Implications:

- Every `useImages`, `useTags`, etc. query runs **once** per cache key and serves the cached value forever (within the 10-min gc window).
- Refetching is opt-in — the only refetch path is explicit `queryClient.invalidateQueries(...)`, which appears in exactly one place (`pages/[...slug].tsx:114` on modal close).
- `staleTime: Infinity` means optimistic updates are the *only* way data becomes fresh after a mutation — there is no background revalidation safety net.
- `retry: false` means a transient IPC failure surfaces immediately as an error rather than retrying.

The `useSemanticSearch` hook overrides these defaults locally (`staleTime: 5min, gcTime: 10min, refetchOnWindowFocus: false`) — semantic results are cached for 5 minutes, after which the same query produces a fresh ONNX inference round trip.

### Routing

```text
src/App.tsx
    ──► <BrowserRouter>
        ──► <QueryClientProvider client={queryClient}>
            ──► <Routes /> via useRoutes(routes)  with routes = "~react-pages"
            ──► <div id="measure-root" />        // off-screen div for DOM measurement (currently unused at runtime)

vite.config.ts
    ──► Pages()  // vite-plugin-pages — auto-generates routes from src/pages/**
```

The catch-all page is `src/pages/[...slug].tsx`. The plugin transforms files in `src/pages/` into a routes config at build time. There is currently exactly one page; adding more would just need new files in `src/pages/`.

### The `measure-root` div

`App.tsx:21-31` mounts an absolutely-positioned, off-screen `<div id="measure-root" />` for DOM-element measurement. The `useMeasure` hook in `src/hooks/useMeasure.tsx` is the consumer — but `useMeasure` is **not imported anywhere** in the current code (verified by grep). The div is dead infrastructure for a feature that didn't ship. It has no runtime cost beyond a single DOM node, but it is misleading.

### Optimistic mutation pattern (the convention)

Three mutations in three files all follow the same shape:

```ts
useMutation({
    mutationFn: (params) => /* async IPC call */,
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
    onSuccess: (data) => { /* swap optimistic placeholder for real data — useCreateTag only */ },
});
```

Locations: `useImages.ts:23-63` (assign), `useImages.ts:65-97` (remove), `useTags.ts:13-46` (create). See `notes/conventions.md` for the canonical conventions doc.

### Type definitions

`src/types.d.ts` exports four types:

| Type | Where it appears | Owner |
|------|------------------|-------|
| `ImageData` | service-layer raw shape (matches Rust `ImageData` JSON) | backend → service translation |
| `ImageItem` | UI shape with `url` and `thumbnailUrl` already constructed via `convertFileSrc` | UI components |
| `Tag` | shared between backend JSON and UI (identical fields) | unified |
| `SimilarImageItem` | similar/semantic results, includes `score` | similarity surfaces |

The translation from `ImageData` to `ImageItem` happens inside `services/images.ts::fetchImages`. The translation from `SemanticSearchResult` JSON to `SimilarImageItem` happens in `services/images.ts::semanticSearch`.

## Key Interfaces / Data Flow

```text
QueryClientProvider context
    ──► useImages, useTags, useSemanticSearch, useTieredSimilarImages — all query hooks share the cache
    ──► useAssignTagToImage, useRemoveTagFromImage, useCreateTag — all mutations follow the optimistic pattern

BrowserRouter
    ──► useRoutes(~react-pages routes)
    ──► pages/[...slug].tsx (the only page)
        ──► uses useLocation/useNavigate to drive the URL-as-selectedItem pattern (see search-routing.md)
```

## Implemented Outputs / Artifacts

- A single `QueryClient` instance shared across the app.
- Auto-generated routes via vite-plugin-pages.
- The `measure-root` DOM node (currently unused).

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `staleTime: Infinity` is unusually aggressive | Any data that changes outside the optimistic-update flow (e.g., a future runtime rescan adds new images) | The cache shows stale data forever. Today's mutations all go through the optimistic pattern, so this is fine; but a runtime rescan would silently fail to surface new images. |
| `searchText` in `useImages` query key | Every keystroke | Cache miss per keystroke for a backend that ignores the field. Wasted memory. |
| `zustand` declared, not used | Build-time | ~78 KB of node_modules. Inherited from earlier memory-bank planning that never landed. |
| `atropos/css` imported in App.tsx | App.tsx:3 | The CSS is imported but the `atropos` runtime is not used (framer-motion does the tilt). The CSS adds a few KB of dead bytes to the bundle. |
| `measure-root` div + `useMeasure` hook | Build-time | Dead infrastructure. Unmounted, unimported. |
| `retry: false` | A transient IPC failure (e.g., DB Mutex briefly unavailable due to a long-running command) | Immediate user-visible error rather than transparent retry. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Drop the unused `zustand`, `atropos`, and `useMeasure` (this would be a single dead-code-sweep PR).
- Drop `searchText` from the `useImages` query key.
- Reconsider the `staleTime: Infinity` default once any data source becomes asynchronous (rescan, watcher, multi-process).
- Optional: a small `retry: 1` for similarity / semantic queries to absorb transient ONNX session.run hiccups.

## Durable Notes / Discarded Approaches

- **TanStack Query was chosen over zustand-as-store.** The original memory-bank notes mentioned zustand for state management; the actual implementation never imported it. The reasoning is implicit in the code: every piece of "state" the app needs is either server data (handled by Query) or per-page React state (handled by `useState`). There is no genuine global store — the URL is the only cross-component shared identifier (the selected image id), and that lives in `react-router`.
- **Aggressive staleTime is intentional given the workload.** Image catalogues do not change without explicit user action (no background fetch, no remote source). With `staleTime: Infinity` the app skips needless re-fetches; mutations handle freshness via the optimistic pattern. The trade-off — silent staleness if data changes outside known mutation paths — is acceptable for a single-user local-first app.
- **The optimistic mutation pattern is the canonical mutation shape**. New mutations should follow the same `cancelQueries → snapshot → optimistic → onError rollback` shape. Documented in `notes/conventions.md`.
- **vite-plugin-pages is overkill for one route** — but it is forward-looking. Adding a settings page or a tag-management page becomes a single-file change. The trade-off is one extra dependency.

## Obsolete / No Longer Relevant

`zustand`, `atropos`, `useMeasure`, the `measure-root` DOM node — all dead, all preserved for now.
