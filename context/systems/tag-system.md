# tag-system

*Maturity: working*

## Scope / Purpose

End-to-end tag CRUD: create / list / delete (now wired) / assign-to-image / remove-from-image, plus two UI surfaces — the search-bar `#`-prefixed autocomplete and the per-image `TagDropdown` inside the modal inspector. All tag mutations use TanStack Query optimistic updates with rollback on error. The grid filter supports both **OR** (any-tag, default) and **AND** (all-tags, opt-in via `tagFilterMode` preference) semantics.

## Boundaries / Ownership

- **Owns:** the 5 tag Tauri commands (delegated via `commands::tags`), the `useTags` / `useCreateTag` / `useDeleteTag` query hooks, the `useAssignTagToImage` / `useRemoveTagFromImage` mutations, the SearchBar `#`-autocomplete, the TagDropdown popover combobox.
- **Does not own:** the actual tag SQL (delegates to `db::tags`), the AND/OR filter SQL (delegates to `db::images_query::get_images_with_thumbnails`).
- **Public API (frontend):** `useTags()`, `useCreateTag()`, `useDeleteTag()`, `useAssignTagToImage()`, `useRemoveTagFromImage()`. Plus the `<SearchBar>` and `<TagDropdown>` components.

## Current Implemented Reality

### Schema (recap)

`tags(id, name UNIQUE, color)` and `images_tags(image_id, tag_id, PRIMARY KEY(...))` with `ON DELETE CASCADE` from both directions. See `systems/database.md`.

### 5 Tauri commands

```
get_tags             () -> Vec<Tag>
create_tag           (name: String, color: String) -> Tag
delete_tag           (tag_id: i64) -> ()                ← NOW WIRED (Phase 6)
add_tag_to_image     (image_id, tag_id) -> ()           ← INSERT OR IGNORE (Phase 6 hardening)
remove_tag_from_image(image_id, tag_id) -> ()
```

`commands/tags.rs`. Returns `Result<T, ApiError>` for all 5.

### `delete_tag` now wired (Phase 6 fix)

Pre-Phase-6, `db::delete_tag` existed in the database layer but was never registered in `invoke_handler!` — orphaned dead code. Phase 6 added the Tauri command + `useDeleteTag` mutation + delete affordance in the search bar / TagDropdown. Typo'd tags can now be removed via UI.

### `add_tag_to_image` is `INSERT OR IGNORE` (Phase 6 hardening)

Pre-Phase-6 it was plain INSERT, which errored with `UNIQUE constraint failed` on duplicate assignment. The frontend pre-checked selection state, but a frontend bug or race condition would surface as a backend error string. The change to `INSERT OR IGNORE` makes duplicates silently no-op.

### Optimistic mutation pattern

Every tag mutation follows the canonical pattern (also documented in `notes/conventions.md`):

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

Specifically:

- `useCreateTag`: optimistic insert with `id = -1` placeholder; `onSuccess` replaces with the real tag from the IPC response.
- `useDeleteTag`: optimistic remove from the `["tags"]` cache; `onError` rolls back.
- `useAssignTagToImage` / `useRemoveTagFromImage`: optimistic mutation of the affected `["images", ...]` query data; `onError` rolls back.

### `#`-prefixed autocomplete

`SearchBar.tsx` reads the input string. When the user types `#`, the SearchBar shows a popover combobox (cmdk) listing matching tags. Selecting one creates a tag pill in the search-bar tag list (`searchTags` state in the page); the input clears. Typing `#newname` then pressing Enter triggers `useCreateTag` to create-on-no-match.

The page's `shouldUseSemanticSearch` excludes `#`-prefixed text so the user doesn't accidentally fire a vector search while picking a tag.

### TagDropdown (per-image)

The PinterestModal renders a `<TagDropdown>` for the selected image, showing currently-assigned tags as pills + an "add tag" combobox. Clicking a pill removes the tag (`useRemoveTagFromImage`); selecting from the combobox assigns (`useAssignTagToImage`). The combobox supports create-on-no-match, same as the SearchBar.

### AND vs OR filter mode

Backend SQL switches based on the `match_all_tags` boolean (defaults to `false` / OR for backwards compatibility):

```sql
-- OR (default): EXISTS-IN
WHERE EXISTS (SELECT 1 FROM images_tags WHERE image_id = images.id AND tag_id IN (...))

-- AND (match_all_tags = true): GROUP BY HAVING COUNT
WHERE images.id IN (
    SELECT it2.image_id FROM images_tags it2
    WHERE it2.tag_id IN (...)
    GROUP BY it2.image_id
    HAVING COUNT(DISTINCT it2.tag_id) = N
)
```

The frontend's `useImages` hook threads `prefs.tagFilterMode === "all"` into the `match_all_tags` IPC argument. The query key includes `matchAllTags` so toggling re-fetches with fresh SQL semantics rather than serving cached OR results.

User-facing toggle: Settings → Search → Tag filter (Any / All).

## Key Interfaces / Data Flow

### Inputs

- User typing in SearchBar with `#` prefix → autocomplete or create flow
- User clicking pill in TagDropdown → assign / remove
- User toggling Settings → Search → Tag filter → AND/OR switch

### Outputs

- `useTags` query → `["tags"]` cache, drives every `<TagDropdown>` and `<SearchBar>` autocomplete
- Mutations write to DB via `commands::tags::*`
- Cache invalidation via `invalidateQueries(["tags"])` and `invalidateQueries(["images"])` after mutations

### Dependencies

- TanStack Query for cache + mutation lifecycle (`systems/frontend-state.md`)
- shadcn `cmdk` primitive (`src/components/ui/command.tsx`) for the combobox
- `framer-motion` for pill animations

## Implemented Outputs / Artifacts

- 5 tag Tauri commands fully exercised by the frontend
- 5 React Query hooks (one per command) following the canonical optimistic pattern
- Two UI surfaces: SearchBar autocomplete + per-image TagDropdown
- AND/OR semantic toggle with frontend pref + backend SQL branch + cache key inclusion

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `create_tag` UNIQUE constraint surfaces as `ApiError::Db("UNIQUE constraint failed: tags.name")` | User creates a tag with an existing name | Frontend gets a typed-but-generic message. Could be sharpened to `ApiError::BadInput("tag already exists")`. |
| Tag color picker doesn't exist | Created tags get a hardcoded default color (`#3489eb`) | Aesthetic limitation — the user can't pick a color when creating. The color column accepts any hex string, so a future picker UI just wires through. |
| AND-filter semantic on a single tag is identical to OR | User selects one tag with AND mode on | The `HAVING COUNT(DISTINCT) = 1` collapses to the same result as `EXISTS-IN`. Cosmetically inefficient SQL but correct. |
| Cache key includes `matchAllTags` | Toggling AND/OR triggers re-fetch | Intentional — caching OR results would show wrong results when toggling. Slightly more network traffic on toggle. |
| Mutation rollback only restores the snapshot, doesn't refetch | Backend rejects the mutation but the cache entry is still consistent | The user sees the optimistic state revert; if the rejection was due to staleness (e.g., another window deleted the tag), the cache might still have the deleted tag. `invalidateQueries` on `onSuccess` covers the success path; `onError` doesn't. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Tag color picker** in the create-tag flow.
- **Sharper `create_tag` errors** for UNIQUE violations (`ApiError::BadInput("tag already exists")`).
- **Bulk tag operations** for multi-select (Phase 10 deferred).
- **Tag rename** — currently a tag is recreated under a new name and re-assigned (manual). A rename command would update in place.

## Durable Notes / Discarded Approaches

- **`INSERT OR IGNORE` over plain INSERT** because duplicate assignment is a no-op user-intent, not an error to propagate. Phase 6 hardening.
- **Optimistic updates with rollback** because TanStack Query's `staleTime: Infinity` means without optimistic updates the UI would feel stale until the next manual refetch. The rollback handles transient IPC failures cleanly.
- **AND/OR is opt-in default-OR** to preserve backwards compatibility for users who had grown used to OR. The toggle lives in Settings rather than a per-search modifier so it's a stable preference, not a per-keystroke decision.
- **Cache key includes `matchAllTags`** so toggling produces a fresh fetch. The alternative — caching one set and filtering client-side — would require fetching every potentially-matching image and is the wrong trade-off for the typical ~10k-image library.
- **Default tag color hardcoded.** A picker is on the roadmap but the default is fine for "I just want to create a tag and move on."

## Obsolete / No Longer Relevant

The pre-Phase-6 orphaned `db::delete_tag` (existed but unreachable from frontend) is gone — now wired through `commands::tags::delete_tag` + `useDeleteTag`. The pre-Phase-6 plain INSERT for `add_tag_to_image` is gone (replaced by INSERT OR IGNORE).
