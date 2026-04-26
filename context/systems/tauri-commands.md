# tauri-commands

*Maturity: comprehensive*

## Scope / Purpose

The IPC surface between the React frontend and the Rust backend. Owns the 22-command handler layer (grouped by concern under `commands/`), the typed `ApiError` discriminated union that flows over the wire, the unified `ImageSearchResult` shape returned by every cosine/semantic command, the lazy text-encoder init in `commands::semantic`, and the `resolve_image_id_for_cosine_path` helper that maps cosine-result paths back to DB ids via three lookup strategies.

This used to be all of `lib.rs` (918 lines). After the audit Modularisation finding it lives in `src-tauri/src/commands/` with one submodule per concern; `lib.rs` is now 232 lines (state types + `run()` + on-Exit perf-report hook only).

## Boundaries / Ownership

- **Owns:** `#[tauri::command]` handler bodies for all 22 commands, the typed-error `ApiError` enum + From-impls + JSON wire shape, `ImageSearchResult` (the unified return type), `resolve_image_id_for_cosine_path` helper, the lazy + pre-warmed text-encoder lifecycle, the cosine cache invalidation pattern on root mutations.
- **Does not own:** SQL (delegates to `db/`), cosine math (delegates to `cosine/`), encoding (delegates to `encoder` and `encoder_text`), path stripping (delegates to `paths::strip_windows_extended_prefix`), the indexing pipeline (delegates to `indexing::try_spawn_pipeline`), settings reading (delegates to `settings::Settings`).
- **Public API (the 22 commands):**

| Command | Inputs | Output | Where |
|---------|--------|--------|-------|
| `get_images` | `filter_tag_ids: Vec<i64>`, `filter_string: String`, `match_all_tags: Option<bool>` | `Vec<ImageData>` | `commands/images.rs` |
| `get_tags` | – | `Vec<Tag>` | `commands/tags.rs` |
| `create_tag` | `name: String`, `color: String` | `Tag` | `commands/tags.rs` |
| `delete_tag` | `tag_id: i64` | `()` | `commands/tags.rs` |
| `add_tag_to_image` | `image_id: i64`, `tag_id: i64` | `()` | `commands/tags.rs` |
| `remove_tag_from_image` | `image_id: i64`, `tag_id: i64` | `()` | `commands/tags.rs` |
| `get_similar_images` | `image_id: i64`, `top_n: usize` | `Vec<ImageSearchResult>` | `commands/similarity.rs` |
| `get_tiered_similar_images` | `image_id: i64` | `Vec<ImageSearchResult>` | `commands/similarity.rs` |
| `semantic_search` | `query: String`, `top_n: usize` | `Vec<ImageSearchResult>` | `commands/semantic.rs` |
| `get_image_notes` | `image_id: i64` | `Option<String>` | `commands/notes.rs` |
| `set_image_notes` | `image_id: i64`, `notes: String` | `()` | `commands/notes.rs` |
| `get_scan_root` | – | `Option<String>` | `commands/roots.rs` (legacy compat) |
| `set_scan_root` | `path: String` | `()` | `commands/roots.rs` (replace-all semantic) |
| `list_roots` | – | `Vec<Root>` | `commands/roots.rs` |
| `add_root` | `path: String` | `Root` | `commands/roots.rs` |
| `remove_root` | `id: i64` | `()` | `commands/roots.rs` |
| `set_root_enabled` | `id: i64`, `enabled: bool` | `()` | `commands/roots.rs` |
| `is_profiling_enabled` | – | `bool` | `commands/profiling.rs` |
| `get_perf_snapshot` | – | `PerfSnapshot` | `commands/profiling.rs` |
| `reset_perf_stats` | – | `Result<(), String>` | `commands/profiling.rs` |
| `export_perf_snapshot` | – | `Result<String, String>` (returns absolute path) | `commands/profiling.rs` |
| `record_user_action` | `action: String`, `payload: serde_json::Value` | `()` | `commands/profiling.rs` |

Every command except the three profiling escape-hatches returns `Result<T, ApiError>`. Handler registration is at `lib.rs:171-194`.

## Current Implemented Reality

### `commands/` submodule layout

```
src-tauri/src/commands/
├── mod.rs        — module re-exports + ImageSearchResult struct + resolve_image_id_for_cosine_path helper
├── error.rs      — pub enum ApiError + Display + std::error::Error + From<rusqlite::Error> +
│                    From<std::io::Error> + From<std::sync::PoisonError<T>> + 5 unit tests
├── images.rs     — get_images
├── tags.rs       — 5 tag commands
├── notes.rs      — get_image_notes, set_image_notes
├── roots.rs      — 6 root + scan-root commands; cosine cache invalidation on every mutation
├── similarity.rs — get_similar_images, get_tiered_similar_images
├── semantic.rs   — semantic_search (with lazy text-encoder fallback)
└── profiling.rs  — 5 profiling escape-hatch commands
```

### `ApiError` typed wire format

```rust
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", content = "details", rename_all = "snake_case")]
pub enum ApiError {
    TokenizerMissing(String),
    TextModelMissing(String),
    ImageModelMissing(String),
    Db(String),
    Encoder(String),
    Cosine(String),
    NotFound(String),
    BadInput(String),
    Io(String),
    Internal(String),
}
```

`commands/error.rs:35-76`. The `#[serde(tag, content, rename_all = "snake_case")]` attribute pins the JSON wire shape. Adding a new variant is forward-compatible: the frontend handles unknown kinds via the default case in its switch.

Wire example:
```json
{ "kind": "tokenizer_missing", "details": "/Users/.../Library/models/tokenizer.json" }
{ "kind": "db", "details": "no such row" }
{ "kind": "encoder", "details": "ONNX session creation failed: ..." }
```

`From`-impls let command bodies use `?` directly:

| `From<X>` | Mapping |
|-----------|---------|
| `From<rusqlite::Error>` | `QueryReturnedNoRows` → `ApiError::NotFound("database row")`; everything else → `ApiError::Db(e.to_string())` |
| `From<std::io::Error>` | → `ApiError::Io(e.to_string())` |
| `From<std::sync::PoisonError<T>>` | → `ApiError::Cosine(format!("mutex poisoned: {e}"))` (the only mutex this crate exposes is the cosine index — the name is slightly imprecise but the intent is clear) |

The `QueryReturnedNoRows → NotFound` mapping lets the frontend branch on `kind === "not_found"` instead of string-matching on the message.

### `ImageSearchResult` — unified shape

```rust
#[derive(serde::Serialize)]
pub struct ImageSearchResult {
    pub id: ID,
    pub path: String,
    pub score: f32,
    pub thumbnail_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
```

`commands/mod.rs:54-65`. Replaces the previous two-struct shape (`SimilarImage` for cosine commands, `SemanticSearchResult` for semantic). All three cosine/semantic commands now return this unified shape — the frontend deserialises one type. Audit "dimensions to backend" finding (`fb23bdb`): width/height are now sourced from the DB row, eliminating the previous DOM Image-load round-trip the frontend used to do per-tile.

### `resolve_image_id_for_cosine_path` — extracted helper

```rust
pub(crate) fn resolve_image_id_for_cosine_path(
    db: &ImageDatabase,
    cosine_path: &Path,
    all_images_cache: Option<&[ImageData]>,
) -> Option<(ID, String)>
```

`commands/mod.rs:90-131`. Three lookup strategies:

1. Strip `\\?\` (via `paths::strip_windows_extended_prefix` — `Cow` return, no alloc on common path), DB lookup using normalised path.
2. DB lookup using the raw cosine path (in case the DB stored the prefixed form).
3. Walk `all_images_cache` comparing under any normalisation (canonicalised, stripped, raw, etc.).

Returns `Some((id, canonical_path))` if any strategy matches; `None` otherwise. Used by `commands::semantic::semantic_search`, `commands::similarity::get_similar_images`, and `commands::similarity::get_tiered_similar_images`. Replaces the previous triplicated inline closure + 60-line duplicated lookup blocks (audit Pattern Extraction finding `02b12b9`).

### Lazy + pre-warmed text encoder

```rust
let mut encoder_lock = text_encoder_state.encoder.lock()?;     // From<PoisonError>

if encoder_lock.is_none() {
    let model_path = paths::models_dir().join("model_text.onnx");
    let tokenizer_path = paths::models_dir().join("tokenizer.json");

    if !model_path.exists()    { return Err(ApiError::TextModelMissing(model_path.display().to_string())); }
    if !tokenizer_path.exists(){ return Err(ApiError::TokenizerMissing(tokenizer_path.display().to_string())); }

    let encoder = TextEncoder::new(&model_path, &tokenizer_path)
        .map_err(|e| ApiError::Encoder(format!("text encoder init failed: {e}")))?;
    *encoder_lock = Some(encoder);
}
```

`commands/semantic.rs:33-62`. The lazy fallback is preserved even though the indexing pipeline pre-warms the encoder — pre-warm can fail (model still downloading on first launch) and the lazy path covers it. If pre-warm succeeded, the `if encoder_lock.is_none()` short-circuits.

The typed `TextModelMissing` / `TokenizerMissing` errors let the frontend's `isMissingModelError(e)` helper trigger a re-download dialog instead of showing a generic toast.

### Cosine cache invalidation on root mutations

Every `commands/roots.rs` mutation clears `cosine_state.index.cached_images`:

```rust
if let Ok(mut idx) = cosine_state.index.lock() {
    idx.cached_images.clear();
}
```

This ensures the next similarity / semantic call rebuilds from the (now-mutated) DB. The `set_scan_root` flow also calls `try_spawn_pipeline` immediately so the cache is repopulated in the background. `add_root` and `remove_root` similarly trigger reindex; `set_root_enabled` does not (the grid query handles it; the cosine cache rebuild happens lazily on the next query).

### State injection pattern

Each command takes the relevant Tauri-managed state via `State<'_, T>`:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_dialog::init())
    .manage(db)                       // ImageDatabase
    .manage(cosine_state)              // CosineIndexState { index: Arc<Mutex<CosineIndex>>, db_path: String }
    .manage(text_encoder_state)        // TextEncoderState { encoder: Mutex<Option<TextEncoder>> }
    .manage(indexing_state.clone())    // Arc<IndexingState> for single-flight
    .manage(watcher_state.clone())     // Arc<Mutex<Option<WatcherHandle>>>
```

`lib.rs:82-89`. The `Arc<IndexingState>` and `Arc<Mutex<Option<WatcherHandle>>>` Tauri-managed states are referenced via `State<'_, Arc<IndexingState>>` — the frontend never sees them, but the commands that mutate roots use them to spawn the indexing pipeline.

### `tracing::instrument` coverage

Every `#[tauri::command]` handler is wrapped with `#[tracing::instrument(name = "ipc.{name}", skip(...))]`. Span names follow the `ipc.{command_name}` convention. Used by the profiling system to surface per-command latency in the perf report.

## Key Interfaces / Data Flow

The full IPC surface is a thin layer that orchestrates state. Per-command flows are documented in `architecture.md` (semantic search) and the relevant system files (`database`, `cosine-similarity`, `clip-text-encoder`, `multi-folder-roots`, `indexing`).

The state-injection pattern + `?`-via-From-impls collapses most command bodies to:

```rust
#[tauri::command]
#[tracing::instrument(name = "ipc.get_tags", skip(db))]
pub fn get_tags(db: State<'_, ImageDatabase>) -> Result<Vec<Tag>, ApiError> {
    Ok(db.get_tags()?)    // From<rusqlite::Error> handles the conversion
}
```

All commands are sync (`fn` not `async fn`). Tauri's invoke handler runs them on its thread pool but each individual command serialises through whatever `Mutex` it acquires.

## Implemented Outputs / Artifacts

- 22 IPC handlers reachable from `invoke()` on the frontend.
- 1 unified `ImageSearchResult` struct returned by every cosine / semantic command.
- 1 typed `ApiError` enum with 10 variants, mirrored on the frontend in `services/apiError.ts`.
- Frontend `formatApiError(unknown)` helper that handles ApiError + legacy strings + Error instances uniformly; `isMissingModelError(e)` predicate for the re-download flow.
- 5 unit tests in `commands/error.rs::tests` pinning the wire format, the rusqlite no-rows special case, and the Display labels.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Three profiling commands return `Result<_, String>` not `Result<_, ApiError>` | Profiling commands existed before the typed-error migration and weren't updated | Frontend's `formatApiError` handles strings via the `instanceof Error` / `String(error)` fallback. Cosmetic inconsistency. |
| `From<PoisonError<T>>` always maps to `ApiError::Cosine` regardless of which mutex was actually poisoned | Any mutex poison anywhere in the codebase | Misleading error label (e.g., a poisoned `TextEncoderState.encoder` shows as "cosine error: mutex poisoned"). Source comment acknowledges this. |
| `add_root` UNIQUE constraint surfaces as generic `ApiError::Db("UNIQUE constraint failed")` | User adds the same folder twice | Could be sharpened to `ApiError::BadInput("already added")` with a specific check. |
| Mutex serialisation across all similarity calls | Concurrent invocations | Cosine `Mutex` is held during the whole sort + sample + result build. Two parallel UI actions queue. Today's UI does not generate parallel calls; a future "preload similar for hovered image" feature would. |
| `assetProtocol.scope: ["**"]` (in `tauri.conf.json`) | A future scenario where untrusted HTML is loaded into the WebView | The current app only loads its own bundled HTML, so this is abstract. For anything more public, scope needs narrowing. See `enhancements/recommendations/08-tauri-csp-asset-scope-hardening.md`. |
| Frontend → backend payload shape evolution | A backend command adds a parameter | Today's frontend services explicitly construct the invoke payload, so adding a parameter on the backend doesn't break old frontend code that omits it (Tauri uses serde defaults), but adding a *required* parameter does. |
| Profiling commands are always registered, even without `--profile` | Frontend calling `record_user_action` on a non-profiling build | The command runs but `perf::record_user_action` short-circuits internally if profiling is off. No-op, no error. |

## Partial / In Progress

- Per-command `tracing::instrument` coverage is complete; per-DB-method coverage (`db.*`) is not. Adding span names like `db.get_image_thumbnail_info` would let the perf report attribute per-DB-method time instead of just per-IPC time. Not yet done.

## Planned / Missing / Likely Changes

- **Sharpen `add_root` errors** — distinguish UNIQUE constraint from other DB errors; surface as `ApiError::BadInput("already added: {path}")`.
- **Migrate the 3 profiling commands to `ApiError`** for consistency.
- **Add a `force_reindex` command** for the case where the user wants to wipe the cache + re-run the pipeline without changing roots. Today this requires `set_scan_root(current_path)` which is awkward.
- **`PoisonError` mapping by source mutex** — could be done via wrapper structs that carry a name through. Today's `From<PoisonError>` is convenient but less precise.

## Durable Notes / Discarded Approaches

- **Tauri commands stay sync, not async.** Tauri 2 supports async commands but every operation in this codebase is naturally synchronous (SQLite calls, mutex-protected mutation, ONNX inference is blocking). Adding `async fn` would force `.await` discipline without buying anything for now. Background work (indexing) lives on its own thread, not in the command body.
- **Lazy `Mutex<Option<TextEncoder>>` was preserved even after pre-warm.** Pre-warm covers the common case; lazy fallback covers the edge cases (pre-warm failed silently because the model was still downloading). The double init protection costs nothing because the lock check `if encoder_lock.is_none()` short-circuits when pre-warm succeeded.
- **`ApiError` over per-command typed errors.** A per-command enum would be more precise but multiplied 22 times. The shared kind allows cross-command branching on the frontend (e.g., "any model-missing error triggers the re-download flow regardless of which command failed").
- **`#[serde(tag, content)]` over a flat shape.** The discriminated-union shape with a string `kind` is what makes the frontend's `switch (e.kind)` work cleanly. Adding a new variant doesn't break the frontend's `default` arm.
- **`?` over `.map_err`.** The `From`-impls remove every per-call `.map_err(|e| ApiError::Db(e.to_string()))` boilerplate. Rust 1.x has had question-mark-with-From for many years; this codebase finally uses it consistently after the typed-error migration.
- **The flexible-match path-resolution fallback was added because Windows path canonicalisation is unstable.** A `\\?\C:\foo\bar` path may or may not match `C:\foo\bar` depending on whether `canonicalize()` succeeds. Strategy 3's flexible match handles the worst case where the path stored at insert time differs from the path in the cosine cache. The deeper fix (normalise-at-insert) is documented in `notes/path-and-state-coupling.md`.
- **Cosine cache invalidation on root mutations is direct, not eventual.** Clearing `cached_images` synchronously before returning means the next user query rebuilds from current DB state. An async invalidation channel was considered and rejected — the synchronous path is simpler and doesn't have race conditions.

## Obsolete / No Longer Relevant

The pre-Phase-6 `lib.rs` (918 lines, all 8 commands inline, `[Backend] ...` println logging, untyped `Result<_, String>` returns) is gone. The `[Backend] ...` logging convention was replaced wholesale by `tracing::info!` / `debug!` / `warn!` during the Phase 6 tracing migration. The previous separate `SimilarImage` and `SemanticSearchResult` structs are replaced by the unified `ImageSearchResult`.
