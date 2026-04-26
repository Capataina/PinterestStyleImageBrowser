# conventions

This file captures patterns that are recurrent in the codebase and not enforced by any tool. New code should follow these unless there is a documented reason to deviate.

## Tracing instrumentation

Every Tauri command, every indexing-pipeline phase, every long-running backend operation gets a `tracing::info_span!` or `#[tracing::instrument]` annotation. Span names follow these prefixes:

| Prefix | Used for | Example |
|--------|----------|---------|
| `ipc.` | `#[tauri::command]` handlers | `#[tracing::instrument(name = "ipc.semantic_search", skip(...))]` |
| `pipeline.` | Indexing pipeline phases | `tracing::info_span!("pipeline.scan_phase").entered()` |
| `cosine.` | Cosine retrieval methods + populate | `#[tracing::instrument(name = "cosine.populate_from_db", skip(...))]` |
| `model_download.` | Model download HTTP work | `model_download.all`, `model_download.head`, `model_download.file` |
| `watcher.` | Filesystem watcher | `watcher.start`, `watcher.event` |

Levels: `info` for spans + state transitions, `debug` for per-result detail (e.g., the top-5 results inside semantic_search), `warn` for non-fatal failures (e.g., a thumbnail decode that fails for one image), `error` for fatal pipeline failures.

The previous `[Backend] ...` `println!` convention is gone. New code should not introduce new `println!`-shaped logging.

The profiling system (`systems/profiling.md`) consumes these spans via `PerfLayer` — adding `#[tracing::instrument]` to a new function automatically gives it perf attribution under `--profile`.

## Domain diagnostics via `record_diagnostic`

Spans answer "how long?". Domain diagnostics answer "what was the system actually doing?" — embedding L2 norms, tokenizer outputs, score distributions, encoder run summaries. The pattern is widespread (17 call sites across `commands/`, `indexing.rs`, `lib.rs`, `cosine/index.rs` as of 2026-04-26):

```rust
crate::perf::record_diagnostic(
    "diagnostic_name",
    serde_json::json!({
        "encoder_id": "siglip2_base",
        "field_a": ...,
        "field_b": ...,
        "interpretation": if condition_a {
            "OK — normalised unit vector"
        } else if condition_b {
            "WARNING — near-zero norm; encoder produced degenerate output"
        } else {
            "BROKEN — NaN/Inf in embedding"
        },
    }),
);
```

Conventions:

- **Diagnostic name** is `snake_case` with no prefix — they are first-class artifacts in the perf report's `## Diagnostics` section.
- **Always include an `encoder_id` field** when the diagnostic is per-encoder so the report can group across all three.
- **Include an `interpretation` field** with a short human-readable verdict (`"OK"` / `"WARNING — ..."` / `"BROKEN — ..."`). The detailed numbers are for follow-up; the interpretation is what someone reading the report scans first to decide whether to dig deeper.
- **No-op when `--profile` absent** — the function returns early without building the JSON. Cheap to call from any code path.
- **Emit at the call site, not via tracing** — diagnostics are richer than fields-on-a-span and fire selectively (per-search, per-cache-load, once-per-session). Use `#[tracing::instrument]` for timing; use `record_diagnostic` for content.

The full diagnostic catalogue lives in `systems/profiling.md` § Domain diagnostics.

## Mutex acquire-then-execute

Every `ImageDatabase` method follows the same shape (~30 lock sites across `db/`):

```rust
self.connection.lock().unwrap().execute("SQL", params)?;
```

The `.unwrap()` is intentional — the project treats Mutex poisoning as unrecoverable; a panic with the lock held should bring down the session and force a restart. See `notes/mutex-poisoning.md`. Match this pattern for new DB methods.

For Tauri command bodies that need to lock cosine / text-encoder / indexing state, use `?` instead of `unwrap`:

```rust
let mut idx = cosine_state.index.lock()?;          // From<PoisonError> in ApiError handles it
```

The `From<PoisonError<T>> for ApiError` impl maps poisoning to `ApiError::Cosine("mutex poisoned: ...")`. The user gets a typed signal instead of a panic.

## Typed errors via `?` and `From`-impls

Every Tauri command returns `Result<T, ApiError>`. Bodies use `?` directly:

```rust
#[tauri::command]
pub fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, ApiError> {
    Ok(db.get_tags()?)   // From<rusqlite::Error> handles the conversion
}
```

`From`-impls in `commands/error.rs` cover:
- `rusqlite::Error` → `ApiError::Db` (with `QueryReturnedNoRows` → `ApiError::NotFound`)
- `std::io::Error` → `ApiError::Io`
- `std::sync::PoisonError<T>` → `ApiError::Cosine("mutex poisoned: ...")`

For specific failure modes that don't map cleanly, construct the variant explicitly:

```rust
return Err(ApiError::TextModelMissing(model_path.display().to_string()));
return Err(ApiError::BadInput(format!("Not a directory: {path}")));
```

The frontend's `services/apiError.ts` mirrors the union and `formatApiError(unknown)` handles ApiError + legacy strings + Error instances uniformly.

The 3 profiling commands (`reset_perf_stats`, `export_perf_snapshot`, `record_user_action`) still use `Result<_, String>` for legacy reasons; not a blocker but should be migrated for consistency.

## Optimistic mutation pattern (frontend)

All TanStack Query mutations follow this shape (~5 occurrences across `useImages.ts`, `useTags.ts`, `useRoots.ts`):

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

Use this exact pattern for any new mutation. The reasoning is in `systems/frontend-state.md` — the `staleTime: Infinity` default makes optimistic updates the only way the UI feels responsive after a mutation, and the rollback handles transient IPC failures.

## `paths::*_dir()` helpers as the single disk-path source

Every file the backend reads or writes goes through a helper in `src-tauri/src/paths.rs`:

| Helper | Returns |
|--------|---------|
| `paths::app_data_dir()` | Root of all app-managed state (Library/ in dev, platform app-data in release) |
| `paths::database_path()` | `app_data_dir / "images.db"` |
| `paths::thumbnails_dir()` | `app_data_dir / "thumbnails"` |
| `paths::thumbnails_dir_for_root(id)` | `thumbnails / "root_<id>"` (Phase 9 reorg) |
| `paths::models_dir()` | `app_data_dir / "models"` |
| `paths::settings_path()` | `app_data_dir / "settings.json"` |
| `paths::cosine_cache_path()` | `app_data_dir / "cosine_cache.bin"` |
| `paths::exports_dir()` | `app_data_dir / "exports"` (perf snapshots, future shareable artefacts) |

Do not hardcode paths. If a new state file is added, add a helper. The dev-vs-release branching in `app_data_dir()` is then transparent to the caller.

## `paths::strip_windows_extended_prefix(&str) -> Cow<'_, str>`

The single helper for stripping `\\?\` prefixes off Windows paths. Returns `Cow::Borrowed` on the common path (no allocation when the prefix is absent). Used by `commands::resolve_image_id_for_cosine_path` for the cosine-path → DB-id mapping fallback.

Do not write inline `if path.starts_with("\\\\?\\") { ... }` closures — that pattern was triplicated pre-audit and the audit explicitly extracted it. The previous `notes/path-and-state-coupling.md` "don't add a fourth normalisation closure" warning is now satisfied by the existence of this helper.

## Submodule layout: `mod.rs` orchestrates, files own concerns

The `db/`, `commands/`, `cosine/`, `encoder_text/` directories follow the same pattern:

```
src-tauri/src/<concern>/
├── mod.rs           — pub use re-exports + the public struct/enum + shared helpers
└── <subconcern>.rs  — impl <Type> { ... } block with the per-subconcern methods + tests
```

Rust merges multiple `impl` blocks for the same type across files in the same crate. The result: `db.add_image(...)` works whether the method is defined in `db/mod.rs` or `db/notes_orphans.rs` — the caller doesn't know.

When adding a new submodule, declare it in `mod.rs` (`mod foo;` or `pub mod foo;`) and add the `impl ImageDatabase { ... }` block in `foo.rs`. Tests for `foo`'s methods live in `#[cfg(test)] mod tests` inside `foo.rs`.

## RAII guards for atomic state

The indexing pipeline's single-flight `AtomicBool` is cleared via an RAII guard:

```rust
struct RunningGuard(Arc<IndexingState>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.is_running.store(false, Ordering::SeqCst);
    }
}
let _guard = RunningGuard(state.clone());
```

Use this pattern when an atomic flag must be cleared on success, error, AND panic. A simple `store(false)` at the end of a function would skip the panic case.

## `lock_result.is_ok()` defensive locking in setup

The lib.rs setup callback and the watcher closure use defensive locking instead of `?`:

```rust
if let Ok(mut slot) = watcher_state.lock() {
    *slot = handle;
}
```

Reason: setup runs early during app launch when error handling is awkward (no IPC channel exists yet to surface failures). Silent failure-to-acquire here is the right trade-off — the app continues launching. Reserve `?` for command bodies where ApiError can flow back to the user.

## `info!` / `debug!` / `warn!` / `error!` levels

| Level | Use for |
|-------|---------|
| `error!` | Pipeline failures that bring down the indexing run |
| `warn!` | Per-image failures (one bad thumbnail), missing-but-non-fatal models, partial scans |
| `info!` | State transitions (pre-warm started, root added, watcher started, populate complete) |
| `debug!` | Per-result detail (top-5 semantic-search results) |

The default env filter is `warn,image_browser_lib=info,image_browser=info`. Without `--profile`, `debug!` lines don't fire.

## File-organisation conventions

- `src-tauri/src/db/` — SQLite layer; one submodule per concern, all impl `ImageDatabase`
- `src-tauri/src/commands/` — Tauri command handlers; one submodule per concern, all `#[tauri::command]`
- `src-tauri/src/similarity_and_semantic_search/` — ML/search subsystem; cosine + encoder + encoder_text are sibling submodules under it
- `src-tauri/src/{indexing,watcher,model_download,perf,perf_report,paths,settings}.rs` — single-file modules at the crate root
- `src/queries/` — TanStack Query hooks; one file per resource family
- `src/services/` — `invoke()` wrappers; one file per resource. Hooks call services; components do not call `invoke` directly.
- `src/components/ui/` — shadcn-generated. Treat as derivative; do not modify by hand.
- `src/components/` — hand-written per-feature components.
- `src/components/settings/` — per-section settings drawer split (Phase 9 + audit Modularisation finding); `index.tsx` is the shell, `*Section.tsx` files are the per-section content.
- `src/hooks/` — utility hooks (debounce, prefs, indexing-progress).

## Naming

- Rust modules and files: `snake_case`.
- Rust types: `PascalCase`.
- TypeScript components: `PascalCase` files (`Masonry.tsx`); hooks and helpers `camelCase` files.
- TypeScript types: `PascalCase`.
- Tauri command names: `snake_case` matching the Rust function name. Frontend invokes via `invoke("get_similar_images", ...)`.
- Tracing span names: `dotted.snake_case` with the prefix conventions above (`ipc.semantic_search`, `pipeline.encode_phase`, `cosine.get_similar_images_sorted`).
- Audit-finding comments: when a piece of code traces back to a specific audit finding, comment it with the commit short-hash:

```rust
// Audit finding (extracted from triplicated inline closures). The project
// notes already flagged "don't add a fourth normalisation closure"
// — the third one was the redundancy.
```

## Test locations

- Backend: `#[cfg(test)] mod tests` inside each submodule. The `db/test_helpers.rs::fresh_db()` helper creates an in-memory DB with `initialize` already run.
- Backend integration: `src-tauri/tests/*.rs` for cross-module tests (currently `cosine_topk_partial_sort_diagnostic.rs`).
- Frontend unit: alongside the source file (`useUserPreferences.test.ts`, `services.test.ts`).
- Frontend component: alongside the source file (`IndexingStatusPill.test.tsx`).

## `pub use submodule::*` re-export pattern

Every concern directory uses `pub use submodule::*` (or selective re-exports) in `mod.rs` to flatten the public API:

```rust
// src/commands/mod.rs
pub use error::ApiError;
pub use images::*;
pub use notes::*;
pub use profiling::*;
// ...
```

This means callers `use crate::commands::ApiError` not `use crate::commands::error::ApiError`. The internal split is invisible at the import level, which keeps refactoring (further splits, renames within the directory) cheap.

## Atomic file save (`tmp` + rename)

When persisting structured data to disk:

```rust
let tmp = path.with_extension("json.tmp");
fs::write(&tmp, content)?;
fs::rename(&tmp, &path)?;
```

Used by `Settings::save`. Survives a crash mid-write — the original file is unchanged until the rename completes. No explicit fsync; sufficient for non-critical state on every modern filesystem the app realistically runs on.

For very-critical state (cosine cache, models) the same pattern applies but isn't currently implemented (the cache uses a single-shot bincode write; a model download writes to `.part` then renames). Worth adding to the cosine cache path in the future.
