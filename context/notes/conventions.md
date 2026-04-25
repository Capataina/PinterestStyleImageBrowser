# conventions

This file captures patterns that are recurrent in the codebase and not enforced by any tool. New code should follow these unless there is a documented reason to deviate.

## Tauri command logging

Every `#[tauri::command]` handler in `lib.rs` logs entry, intermediate state, and exit using the `[Backend]` prefix:

```rust
println!("[Backend] semantic_search called - query: '{}', top_n: {}", query, top_n);
// ... work ...
println!("[Backend] semantic_search returning {} results", results.len());
```

There are 16 such call sites. The convention is consistent enough that any new command should follow it without needing further direction.

**Open question:** the convention should migrate to `tracing::info!` / `tracing::debug!` when `tracing` is added. Until then, `println!` is the standard.

## Mutex acquire-then-execute

Every `ImageDatabase` method follows the same shape (20 occurrences in `db.rs`):

```rust
self.connection.lock().unwrap().execute("SQL", params)?;
```

A more defensive form (`.lock().map_err(...)?`) would handle poisoning. The current code unwraps because the project treats Mutex poisoning as unrecoverable — a panic with the lock held should bring down the session. Match this pattern for new DB methods.

## IPC error mapping

Tauri command bodies wrap their internal `Result<_, _>` with `.map_err(|e| e.to_string())` before returning. This is the IPC boundary's error-erasure pattern — typed errors do not survive the JSON serialisation. 5 occurrences in `lib.rs`.

If you need richer error information at the frontend, the right intervention is to define a typed error enum in `lib.rs` that derives `Serialize` and use `Result<T, ApiError>` for the command return. Today this is not done.

## Optimistic mutation pattern (frontend)

All TanStack Query mutations follow this shape (3 occurrences across `useImages.ts` and `useTags.ts`):

```ts
useMutation({
    mutationFn: (params) => /* IPC call */,
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

Use this exact pattern for any new mutation. The reasoning is in `systems/frontend-state.md` — the `staleTime: Infinity` default makes optimistic updates the only way the UI feels responsive after a mutation, and the rollback handles transient IPC failures.

## `rand::rng()` for stochastic UX

Three places in the backend use random sampling: the catalog shuffle (`db.rs:498`), the diversity-pool sampler (`cosine_similarity.rs:151`), and the tiered sampler (`cosine_similarity.rs:291`). All three use `rand::rng()` — the new thread-local API in `rand 0.9`.

Don't switch back to `thread_rng()` (the older API) — the project is on `rand = 0.9.2` and the new API is consistent.

## Naming

- Rust modules and files: `snake_case` (`cosine_similarity.rs`, `encoder_text.rs`).
- Rust types: `PascalCase` (`ImageDatabase`, `CosineIndex`, `TextEncoder`).
- TypeScript components: `PascalCase` files for components (`Masonry.tsx`, `PinterestModal.tsx`), `camelCase` for hooks and helpers.
- TypeScript types: `PascalCase` (`ImageItem`, `Tag`).
- Tauri command names: `snake_case` matching the Rust function name (e.g., `get_similar_images`, not `getSimilarImages`). The frontend calls them by their Rust name via `invoke("get_similar_images", ...)`.

## File-organisation conventions

- `src/queries/` holds TanStack Query hooks; one file per resource family (`useImages.ts`, `useTags.ts`).
- `src/services/` holds the `invoke()` wrappers — translates Tauri JSON into UI types. One file per resource (`images.ts`, `tags.ts`). Hooks call services; components do not call `invoke` directly.
- `src/components/ui/` is shadcn-generated. Treat as derivative; do not modify by hand.
- `src/components/` is hand-written. Per-feature components (`Masonry.tsx`, `SearchBar.tsx`, `PinterestModal.tsx`).
- `src-tauri/src/similarity_and_semantic_search/` is a sub-module. The three files (`cosine_similarity.rs`, `encoder.rs`, `encoder_text.rs`) cluster because they form the ML/search subsystem; `mod.rs` re-exports them.
