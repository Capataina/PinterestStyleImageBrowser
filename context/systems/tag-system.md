# tag-system

*Maturity: working*

## Scope / Purpose

End-to-end tag CRUD: create / list / assign-to-image / remove-from-image, plus two UI surfaces — the search-bar `#`-prefixed autocomplete and the per-image `TagDropdown` inside the modal inspector. All tag mutations use TanStack Query optimistic updates with rollback on error.

## Boundaries / Ownership

- **Owns:** tag UI (`SearchBar.tsx`, `TagDropdown.tsx`), tag query hooks (`useTags.ts`), the optimistic mutation pattern for tag assignment / removal (`useImages.ts`).
- **Does not own:** the image grid filter SQL (lives in `database`), the search routing priority (lives in `search-routing`).
- **Public API:** `useTags()`, `useCreateTag()`, `useAssignTagToImage()`, `useRemoveTagFromImage()`, `<SearchBar>`, `<TagDropdown>`.

## Current Implemented Reality

### Schema (recap from `database`)

- `tags(id, name, color)` with `name` UNIQUE.
- `images_tags(image_id, tag_id)` join with composite PK and `ON DELETE CASCADE`.

### Default tag colour

`#3489eb` is the default in `services/tags.ts:15`. The `TagDropdown` and `SearchBar` create-on-no-match path passes `#3B82F6` instead (`SearchBar.tsx:98`, `TagDropdown.tsx:68`). These are Tailwind blues but they are not the same blue — minor inconsistency worth resolving.

### Two creation surfaces

| Surface | Trigger | Code |
|---------|---------|------|
| `SearchBar` `#`-prefix autocomplete | User types `#newtag` and presses Enter or clicks "Create" | `SearchBar.tsx:67-101` |
| `TagDropdown` inside `PinterestModal` | User opens dropdown and types a non-existent tag name | `TagDropdown.tsx:38-87` |

Both call the same `useCreateTag` mutation, which optimistically inserts a placeholder tag with `id: -1` then swaps it for the server-assigned id on success.

### Optimistic update pattern

The pattern repeats in three mutations:

```text
onMutate(params):
    cancelQueries(["images" or "tags"])       // pause in-flight queries
    prevData = queryClient.getQueryData(...)  // snapshot
    queryClient.setQueriesData(...)           // apply optimistic change
    return { prevData }                       // saved for rollback
onError(err, vars, context):
    if context?.prevData:
        queryClient.setQueryData(..., prevData)  // restore snapshot
onSuccess(data):  (only useCreateTag)
    swap the id=-1 placeholder for the real server id
```

Source: `useImages.ts:23-97`, `useTags.ts:13-46`. See `notes/conventions.md` for the canonical conventions doc.

### Tag filter (read path)

The `<SearchBar>` keeps a `selectedTags: Tag[]` state. When the user adds a tag (via Enter on the autocomplete), the search bar emits `onSearchChange(selectedTags, searchText)` to the parent. The parent feeds `tagIds` into `useImages({ tagIds: selectedTags.map(t => t.id), searchText })`, which becomes the `filter_tag_ids` argument to `db.get_images_with_thumbnails`.

The SQL is OR-semantic — see `systems/database.md` for details.

### Tag removal

Two paths:
- **From the search bar:** click the `RxCrossCircled` icon on a tag pill (`SearchBar.tsx:153-158`). Removes the tag from the selected set; does **not** delete the tag from the DB.
- **From the modal:** click the `RxCrossCircled` icon on a tag pill (`PinterestModal.tsx:144-148`). Calls `props.onRemoveTag(imageId, tagId)` which fires the `useRemoveTagFromImage` mutation. Removes the tag from this image; does **not** delete the tag from the DB.

There is no UI path to delete a tag from the catalogue. `db.delete_tag` exists but is unwired (see `tauri-commands.md`).

## Key Interfaces / Data Flow

```text
SearchBar (typing # mode)
    ──► onSearchChange(selectedTags, "")  ──► parent state
                            └─► useImages({ tagIds, searchText: "" })  ──► invoke("get_images")

PinterestModal
    ──► TagDropdown
        ──► toggle: useAssignTagToImage / useRemoveTagFromImage
            └─► invoke("add_tag_to_image" / "remove_tag_from_image")

useCreateTag
    ──► invoke("create_tag", { name, color })
    └─► optimistic insert with id=-1, swap on success
```

## Implemented Outputs / Artifacts

- Tag pills in `SearchBar` and `PinterestModal`.
- Optimistic updates that rarely flicker — the user sees the tag appear immediately, with rollback only if the IPC call fails.
- A growing `tags` table in `images.db`.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Cannot delete tags via UI | A typo'd tag created via either creation surface | Persistent typo in the tag list. No way to clean up. |
| Default tag colour inconsistency | Creation via `services/tags.ts::createTag` directly vs via the UI's `SearchBar`/`TagDropdown` (which override with `#3B82F6`) | Tags created from different code paths get different default colours. Minor visual noise. |
| `add_tag_to_image` is `INSERT` not `INSERT OR IGNORE` | Frontend bug (or future caller) assigning the same tag twice | Backend errors with a UNIQUE-constraint violation. The optimistic update flickers (UI shows the tag, then onError rolls back). |
| Tag UNIQUE constraint on `name` | Creating a tag with the same name as an existing one | Backend errors. The frontend create flow does check for `exactMatch` but a race could allow a duplicate to be attempted. |
| OR-semantic filter, not AND | A user expecting "landscape AND sunset" | They get OR-semantics with no UI signal. README ambiguous on this. |
| Search-bar tag-suggestion popover positioning | Long search text with many selected tags | The PopoverAnchor is anchored to a relative-positioned wrapper; on small viewports the popover can clip. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Wire `delete_tag` as a Tauri command and add a delete affordance in `TagDropdown` (probably a small `X` next to each tag in the dropdown list with confirmation).
- Resolve the colour inconsistency: pick one default and use it everywhere.
- Decide AND vs OR semantics and either document or extend the SQL to support both.
- Per-tag colour picker in the create-tag UI (today the colour is hardcoded blue).

## Durable Notes / Discarded Approaches

- **The `id: -1` placeholder for optimistic creation is deliberate.** Real ids are SQLite rowids (positive integers), so `-1` is unambiguously an optimistic placeholder. The `onSuccess` swap by `id === -1` is reliable as long as only one create mutation is in-flight at a time. With concurrent creates (which the UI does not allow today), the swap could match the wrong placeholder.
- **The `#`-prefix UX in `SearchBar` is the entire tag-filter affordance.** There is no separate "filter by tag" button. The `#` syntax is taught implicitly via the placeholder text "Search images or type # to filter by tags...". This is a deliberate single-input UX choice — the search bar is both semantic and tag-filter at once, with the prefix disambiguating intent. The shape is good; what is not yet good is the error feedback when the `#` doesn't match anything.
- **The filter is OR-semantic intentionally for the current use case.** Per cross-vault analysis, the README implies AND/OR support but the code only does OR. The author has not stated which is "correct" — a decision is owed.

## Obsolete / No Longer Relevant

None.
