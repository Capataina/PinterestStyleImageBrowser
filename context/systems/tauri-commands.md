# tauri-commands

*Maturity: working*

## Scope / Purpose

The IPC surface between the React frontend and the Rust backend. Owns: state management lifecycle (`tauri::Builder::manage`), Windows-extended-path normalisation, and the multi-strategy fallback that maps cosine-result paths back to DB ids. Eight command handlers live in this layer.

## Boundaries / Ownership

- **Owns:** `#[tauri::command]` handler bodies, `State<'_, ImageDatabase>`/`State<'_, CosineIndexState>`/`State<'_, TextEncoderState>` injection, `map_err(|e| e.to_string())` boundary, the path-normalisation closure (currently triplicated).
- **Does not own:** SQL (delegates to `database`), cosine math (delegates to `cosine-similarity`), encoding (delegates to the encoder modules).
- **Public API (the 8 commands):**

| Command | Inputs | Output | Implementation |
|---------|--------|--------|----------------|
| `get_images` | `filter_tag_ids: Vec<i64>`, `filter_string: String` | `Vec<ImageData>` | Delegates to `db.get_images_with_thumbnails`. `filter_string` is unused on the backend; preserved as cache-key on the frontend. |
| `get_tags` | – | `Vec<Tag>` | `db.get_tags()` |
| `create_tag` | `name: String`, `color: String` | `Tag` | `db.create_tag(name, color)` |
| `add_tag_to_image` | `image_id: i64`, `tag_id: i64` | `()` | `db.add_tag_to_image(...)`. `INSERT` (not `OR IGNORE`) — duplicate assignment errors. |
| `remove_tag_from_image` | `image_id: i64`, `tag_id: i64` | `()` | `db.remove_tag_from_image(...)` |
| `get_similar_images` | `image_id: i64`, `top_n: usize` | `Vec<SimilarImage{id, path, score}>` | Cosine `get_similar_images` with diversity sampling; multi-strategy DB-id mapping. |
| `get_tiered_similar_images` | `image_id: i64` | `Vec<SimilarImage>` | Cosine `get_tiered_similar_images` (7-tier); multi-strategy DB-id mapping. |
| `semantic_search` | `query: String`, `top_n: usize` | `Vec<SemanticSearchResult{id, path, score, thumbnail_path?, width?, height?}>` | Lazy-init text encoder → `cosine.get_similar_images_sorted` → enrich with thumbnail info. |

Source: `lib.rs:49-621`. Handler registration is at `lib.rs:612-621`.

## Current Implemented Reality

### Three Tauri-managed state objects

```rust
tauri::Builder::default()
    .manage(db)                  // ImageDatabase
    .manage(cosine_state)        // CosineIndexState  { index: Mutex<CosineIndex>, db_path: String }
    .manage(text_encoder_state)  // TextEncoderState  { encoder: Mutex<Option<TextEncoder>> }
```

`lib.rs:597-611`. Each command takes the state via `State<'_, T>`.

### Lazy text-encoder init

The text encoder is the heaviest resource (an ONNX session + a tokenizer vocab). It is created lazily on first semantic search:

```rust
if encoder_lock.is_none() {
    if !model_path.exists() { return Err(format!("Text model not found at: {}", ...)); }
    if !tokenizer_path.exists() { return Err(format!("Tokenizer not found at: {}", ...)); }
    *encoder_lock = Some(TextEncoder::new(model_path, tokenizer_path)?);
}
```

`lib.rs:114-142`. The `Option` wrapper is what allows a `Mutex<Option<...>>` to be created at app start without paying the model-load cost upfront. A side effect is that a missing `tokenizer.json` only manifests as an error on the first semantic-search query, not at app startup.

### Path normalisation — the triplicated closure

The cosine module returns `Vec<(PathBuf, f32)>`. Mapping `PathBuf` back to a DB id requires the path to look identical to whatever the DB stored — but the DB may have stored a regular path while cosine constructed it with a Windows-extended `\\?\` prefix.

The closure that strips that prefix is **defined inline** inside three commands:

- `semantic_search` at `lib.rs:182-188`
- `get_similar_images` at `lib.rs:467-475`
- `get_tiered_similar_images` at `lib.rs:308-315`

```rust
let normalize_path = |path_str: &str| -> String {
    if path_str.starts_with("\\\\?\\") {
        path_str[4..].to_string()
    } else {
        path_str.to_string()
    }
};
```

Three exact copies. A future refactor should extract this to a module-level helper. The deeper fix is to normalise paths at insert time (in the filesystem-scanner) so the strip is never needed.

### Multi-strategy DB-id mapping

After cosine returns a path, the command tries to map it to an `images.id` row using up to three strategies:

1. **Normalised path lookup** — `db.get_image_id_by_path(&normalize_path(path_str))`.
2. **Original path lookup** — `db.get_image_id_by_path(&path_str)` (in case the DB stored the `\\?\` prefix).
3. **Flexible match against all images** — fetch `db.get_all_images()` and find one whose path matches under either normalisation or canonicalisation.

Implementation: `get_similar_images` at `lib.rs:484-573`. `get_tiered_similar_images` at `lib.rs:319-373`. `semantic_search` uses a slightly simpler 3-strategy variant at `lib.rs:194-221`.

This was added in commit `2606854` (2025-12-12): "Enhanced the logic for mapping image paths to IDs by normalizing Windows extended path prefixes, attempting both normalized and original formats, and adding a flexible fallback that compares canonicalized paths against all images in the database. This increases robustness when handling various path representations and improves matching accuracy."

### `[Backend] ...` logging convention

Every command logs its inputs, intermediate states, and outputs via `println!("[Backend] ...")`. There are 16 such calls across `lib.rs`. The convention is:

- One log on entry: `[Backend] {command} called - {key args}`.
- Logs on cache state transitions: `[Backend] Cache is empty, populating from database...`.
- Logs on output mapping (per-result file name + score for the first 5 results).
- One log on exit: `[Backend] {command} returning N results`.

This is excellent for development but unsuitable for production. Replacing `println!` with `tracing::info!`/`tracing::debug!` and gating verbose logs by env var is the obvious next step.

### `map_err(|e| e.to_string())` at the IPC boundary

Five Tauri commands wrap their internal `Result<_, _>` with `.map_err(|e| e.to_string())` before returning. Errors crossing the IPC boundary are stringified — typed errors are erased.

Frontend consequence: `services/images.ts` catches and re-throws with a generic `"Search failed: ..."` message. The actual cause (tokenizer missing, CUDA init, DB lock contention) is lost on the wire.

## Key Interfaces / Data Flow

The full IPC surface is a thin layer that orchestrates state. Per-command flows are documented in `architecture.md` (semantic search) and the relevant system files (`database`, `cosine-similarity`, `clip-text-encoder`).

The state-injection pattern:

```text
tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .manage(db)
    .manage(cosine_state)
    .manage(text_encoder_state)
    .invoke_handler(tauri::generate_handler![ ...8 commands... ])
    .run(tauri::generate_context!())
```

All commands are sync (`fn` not `async fn`). Tauri's invoke handler runs them on its thread pool but each individual command serialises through whatever `Mutex` it acquires.

## Implemented Outputs / Artifacts

- 8 IPC handlers reachable from `invoke()` on the frontend.
- Two ad-hoc result types: `SimilarImage { id, path, score }` and `SemanticSearchResult { id, path, score, thumbnail_path?, width?, height? }` (`lib.rs:13-28`). The latter is richer because the frontend wants thumbnails directly without a follow-up `get_image_thumbnail_info` round trip.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `delete_tag` exists in `db.rs` but is **not registered** in `invoke_handler!` | Wanting to remove a tag via UI | No code path can call it from the frontend. `db.delete_tag` is dead code from the IPC surface's perspective. |
| Triplicated `normalize_path` closure | Future schema or path-format change | Three places to edit; easy to update one and forget the others. The flexible-match fallback uses additional inline canonicalisation that is also duplicated. |
| String-stringified errors | Any error in any command | Frontend cannot distinguish "model file missing" from "tokenizer parse failed" from "CUDA init error" from "Mutex poisoned" — all surface as generic "Search failed". |
| `println!` in hot path | Every command call | stdout grows unbounded in dev; production builds also log. No log level control. |
| Mutex serialisation across all similarity calls | Concurrent invocations | Cosine `Mutex` is held during the whole sort + sample + result build. Two parallel UI actions queue. Today's UI does not generate parallel calls, but a future "preload similar for hovered image" feature would. |
| Hardcoded model-file paths | A future `models/` move | The `models/model_text.onnx` and `models/tokenizer.json` paths are hardcoded inside `semantic_search` (`lib.rs:121-122`). |
| `assetProtocol.scope: ["**"]` (in `tauri.conf.json`) | A future scenario where untrusted HTML is loaded into the WebView | The current app only loads its own bundled HTML, so this is abstract. For anything more public, scope needs narrowing. |

## Partial / In Progress

None active.

## Planned / Missing / Likely Changes

- Register `delete_tag` as a Tauri command + add a delete affordance in `TagDropdown`.
- Add a `scan_directory(path)` command that triggers the full filesystem → DB → thumbnails → encoding pipeline at runtime, paired with a Tauri `dialog` plugin invocation on the frontend.
- Add a `rescan_directory()` companion command for re-indexing without restart.
- Replace `println!` with `tracing` + `tracing-subscriber`.
- Extract `normalize_windows_path` into a module-level helper; eventually move the normalisation to insert time.
- Surface real error strings to the frontend (preserve `.to_string()` of typed errors with their context).

## Durable Notes / Discarded Approaches

- **The lazy `Mutex<Option<TextEncoder>>` pattern was a deliberate choice.** Eager init at app startup would add several seconds to launch even for users who never run a semantic search. Lazy init defers the cost until the first search and pays it once. A `OnceLock` would be a cleaner spelling but does not interact well with the `Result<TextEncoder, ...>` constructor.
- **Tauri commands are sync, not async.** Tauri 2 supports async commands but every operation in this codebase is naturally synchronous (SQLite calls, mutex-protected mutation, ONNX inference is blocking). Adding `async fn` would force `.await` discipline without buying anything for now.
- **The flexible-match fallback was added because Windows path canonicalisation is unstable.** A `\\?\C:\foo\bar` path may or may not match `C:\foo\bar` depending on whether `canonicalize()` succeeds (which depends on the file existing on disk *right now*). The fallback walks all images comparing under multiple normalisations to handle the worst case where the path stored at insert time differs from the path in the cosine cache.

## Obsolete / No Longer Relevant

None.
