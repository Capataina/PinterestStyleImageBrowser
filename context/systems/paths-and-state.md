# paths-and-state

*Maturity: working*

## Scope / Purpose

Single source of truth for every disk path the app uses. Provides the small handful of helpers (`paths::*_dir()`) that every other system reads. Also owns the `Settings` struct that persists user-managed knobs (`scan_root`, `priority_image_encoder` legacy field, `enabled_encoders` Phase 11c).

This is a small but load-bearing module: every system that reads or writes state goes through here. A bug in `paths::app_data_dir()` would put state files in the wrong place silently.

## Boundaries / Ownership

- **Owns:** `paths::app_data_dir()`, `paths::database_path()`, `paths::thumbnails_dir()`, `paths::thumbnails_dir_for_root(id)`, `paths::models_dir()`, `paths::settings_path()`, `paths::cosine_cache_path()`, `paths::exports_dir()`, `paths::strip_windows_extended_prefix(&str) -> Cow<'_, str>`, the `Settings` struct + its load/save methods + `resolved_enabled_encoders()` helper.
- **Does not own:** the file contents themselves (each owning system writes its own format), the app-data directory itself (created on first call to `app_data_dir`), the bundle-id (just stores a constant for the platform-default fallback).
- **Public API (paths):** see Owns above.
- **Public API (settings):** `Settings::default()`, `Settings::load() -> Self`, `Settings::save(&self) -> io::Result<()>`, `Settings::resolved_enabled_encoders() -> Vec<String>`.

## Current Implemented Reality

### App-data layout

Every state file lives under one root, the platform's standard app-data directory. Same layout in dev and release — there is **no `cfg(debug_assertions)` branching anymore**. (See "What changed" below for why.)

```
<app_data_dir>/                       # platform-standard, see paths.rs:81 for resolution
  images.db                           # SQLite (WAL adds .db-wal + .db-shm)
  settings.json                       # scan_root + enabled_encoders + (legacy) priority_image_encoder
  cosine_cache.bin                    # bincode-encoded Vec<(PathBuf, Vec<f32>)>
  models/
    clip_vision.onnx                  # CLIP image (~352 MB)
    clip_text.onnx                    # CLIP text (~254 MB)
    clip_tokenizer.json               # CLIP BPE
    dinov2_base_image.onnx            # DINOv2 base (~347 MB)
    siglip2_vision.onnx               # SigLIP-2 image (~372 MB)
    siglip2_text.onnx                 # SigLIP-2 text (~1.13 GB)
    siglip2_tokenizer.json            # Gemma SentencePiece
  thumbnails/
    root_<id>/                        # one subfolder per root (Phase 9)
      thumb_<image_id>.jpg
    thumb_<image_id>.jpg              # legacy flat layout for root_id = NULL rows
  exports/
    perf-<unix_ts>/
      timeline.jsonl                  # raw event stream from PerfLayer
      report.md                       # rendered on app exit
      raw.json                        # snapshot at exit
    perf-<unix_ts>.json               # one-off snapshot from "Export" button
```

Where `<app_data_dir>` resolves to (in order):
1. `$IMAGE_BROWSER_DATA_DIR` if set and non-empty (env-var override for testing / multi-instance / CI fixtures)
2. `dirs::data_dir()/com.ataca.image-browser/` — the platform default:
   - **macOS:** `~/Library/Application Support/com.ataca.image-browser/`
   - **Linux:** `$XDG_DATA_HOME/com.ataca.image-browser/` (typically `~/.local/share/...`)
   - **Windows:** `%APPDATA%/com.ataca.image-browser/`
3. `./app-data/com.ataca.image-browser/` if `dirs::data_dir()` returns `None` (rare — only on stripped-down environments where the standard data dir can't be resolved). Logged at warn level.

### Why the dev-vs-release split was removed

An earlier version of this module branched on `cfg(debug_assertions)`: dev builds wrote to `<repo>/Library/`, release used the platform default. The split was removed because dev and release builds diverged on every code change, forcing the user to re-download all 2.5 GB of models whenever they switched build modes. Now both share state — the comment in `paths.rs::app_data_dir` explicitly cites this as the trigger for reverting.

The user can still sandbox a session via `IMAGE_BROWSER_DATA_DIR=/some/tmp/path` if they want isolation (the env-var override is the supported alternative to the old dev-path branching).

### Per-root thumbnail subdirs

```rust
pub fn thumbnails_dir_for_root(root_id: i64) -> PathBuf {
    let p = thumbnails_dir().join(format!("root_{root_id}"));
    let _ = ensure_dir(&p);
    p
}
```

Phase 9 reorganisation: `remove_root` can `rm -rf` the per-root subfolder cleanly, instead of per-row file deletion. Legacy `root_id = NULL` rows continue writing to the flat `thumbnails_dir()` path.

### Windows path stripping

```rust
pub fn strip_windows_extended_prefix(path_str: &str) -> Cow<'_, str> {
    match path_str.strip_prefix("\\\\?\\") {
        Some(stripped) => Cow::Owned(stripped.to_string()),
        None => Cow::Borrowed(path_str),
    }
}
```

Used by `commands::resolve_image_id_for_cosine_path` to map cosine-result paths back to DB ids when the canonical form drifts (Windows-extended-prefix vs not). The `Cow` return means non-Windows paths pay zero allocation for the common case where the prefix isn't present. Switched from manual slice indexing to `strip_prefix` in Phase 6 (clippy gate).

### Per-root thumbnail subdirs

```rust
pub fn thumbnails_dir_for_root(root_id: i64) -> PathBuf {
    let p = thumbnails_dir().join(format!("root_{root_id}"));
    let _ = ensure_dir(&p);
    p
}
```

Phase 9 reorganisation: `remove_root` can `rm -rf` the per-root subfolder cleanly, instead of per-row file deletion. Legacy `root_id = NULL` rows continue writing to the flat `thumbnails_dir()` path.

### `Settings` struct (current shape)

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub scan_root: Option<PathBuf>,

    /// LEGACY (Phase 11c) — was the user's "primary" image encoder
    /// when the picker was a single-choice dropdown. Now obsolete:
    /// fusion uses every enabled encoder. Kept on the struct so old
    /// settings.json files deserialise without erroring.
    #[serde(default)]
    pub priority_image_encoder: Option<String>,

    /// Phase 11c — encoder ids the user has enabled. The indexing
    /// pipeline only encodes for these; image-image and text-image
    /// fusion only fuse over these. None = use the default set
    /// (every supported encoder enabled).
    #[serde(default)]
    pub enabled_encoders: Option<Vec<String>>,
}
```

The `#[serde(default)]` on each field means newer binaries reading an older `settings.json` deserialise cleanly to defaults; older binaries reading a newer `settings.json` just ignore the unknown fields. Persisted atomically via write-to-`.tmp` + rename.

`Settings::resolved_enabled_encoders()` returns the user's pick when set, falling back to `DEFAULT_ENABLED_ENCODERS = ["clip_vit_b_32", "siglip2_base", "dinov2_base"]` when None or empty. Empty-list is treated as "use default" not "disable all" — the IPC validator (`commands::encoders::decide_enabled_write`) also rejects empty mutations, so the empty-set guard is belt-and-braces.

The `scan_root` field is the historical pre-multi-folder migration target. The `lib.rs::run::setup` callback consumes it once on first launch under the multi-folder schema, then clears it so the migration doesn't re-trigger.

Frontend `useUserPreferences` (theme, columns, sortMode, animation, similar/semantic result counts, tagFilterMode, legacy imageEncoder/textEncoder ids) lives separately in `localStorage`. The two stores don't overlap: backend Settings governs persistent per-install behaviour the indexing pipeline cares about; frontend prefs govern UI taste the backend doesn't see.

## Key Interfaces / Data Flow

### Read sites

| Caller | Function | Purpose |
|--------|----------|---------|
| `db::ImageDatabase::default_database_path` | `database_path()` | Open the SQLite file (writer + read-only secondary R2) |
| `lib.rs::run::setup` | `Settings::load()` | Check for legacy scan_root |
| `lib.rs::run::setup` | `Settings::save()` | Clear scan_root after legacy migration |
| `indexing.rs::run_pipeline_inner` | `models_dir().join(...)` | Verify model files exist |
| `indexing.rs::run_pipeline_inner` | `Settings::load().resolved_enabled_encoders()` | Pick which encoders to spawn parallel threads for (Phase 11c) |
| `indexing.rs::run_pipeline_inner` | `thumbnails_dir()` | Pass to ThumbnailGenerator::new |
| `indexing.rs::run_pipeline_inner` | `cosine_cache_path()` (indirectly via `cosine.save_to_disk`) | Persist cosine cache |
| `model_download.rs::download_models_if_missing` | `models_dir()` | Where to write downloads |
| `commands::semantic::semantic_search` (legacy) | `models_dir()` | Lazy-init text encoder for the single-encoder fallback path |
| `commands::semantic_fused::get_fused_semantic_search` | `models_dir()` + `Settings::load()` | Lazy-init enabled text encoders for RRF fusion (Phase 11d) |
| `commands::similarity::get_fused_similar_images` | `Settings::load().resolved_enabled_encoders()` | Iterate over enabled encoders for image-image RRF fusion (Phase 5) |
| `commands::encoders::{get,set}_enabled_encoders` | `Settings::{load,save}` | Per-encoder toggle persistence |
| `commands::roots::remove_root` | `thumbnails_dir_for_root(id)` | rm -rf the per-root subfolder |
| `commands::profiling::export_perf_snapshot` | `exports_dir()` | Write one-off perf snapshots |
| `commands::resolve_image_id_for_cosine_path` | `strip_windows_extended_prefix(...)` | Path lookup fallback |
| `main.rs` | `paths::exports_dir()` (via `perf::init_session`) | Initialize profiling session dir (only when `--profiling` flag or `PROFILING=1` env var is set) |

### Write sites

`paths` itself doesn't write — `ensure_dir` is the only filesystem mutation, called transitively from every `*_dir()` accessor as a side effect of returning the path. Callers do all the actual file writes.

## Implemented Outputs / Artifacts

- Compile-time-stable `Library/` layout, identical in dev and release modulo where the parent lives.
- A small set of pure functions (no IO beyond `ensure_dir`) that every other system depends on.
- 6 unit tests in `paths.rs::tests` pinning the layout (dir basenames, per-root subfolder creation, file-name stability).
- Atomic save for `settings.json` (`.tmp` + rename pattern) — survives partial-write failure.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| `dirs::data_dir()` returning `None` in release | A platform without an XDG-or-equivalent data dir | Falls back to `./app-data` (relative to cwd) and logs a warn. The user might end up writing to wherever they launched the app from. macOS/Linux/Windows all have `dirs::data_dir()` support, so this is mostly theoretical. |
| `CARGO_MANIFEST_DIR` is wrong | Build environment not set up via Cargo (e.g., a custom build script) | The dev path falls back to `./Library/` (cwd). Untested but unlikely to bite given `cargo tauri dev` is the only real dev launcher. |
| `ensure_dir` failure swallowed | Filesystem full or permissions error | Returns the path anyway; subsequent file open fails. Gives a confusing error message that doesn't hint at the directory creation failure. |
| Settings.json corruption | Manual edit + invalid JSON | `Settings::load` logs error and returns `Settings::default()` — silently drops the legacy `scan_root` field. The migration path won't fire. Acceptable. |
| Atomic save uses `rename` not `fsync` | Power loss between `write` and `rename` | The `.tmp` file may exist on disk; on next launch settings.json is unchanged. If the rename succeeded but the directory entry didn't fsync, the new file may be partial. macOS / Linux ext4 / Windows NTFS handle this well in practice but no explicit fsync. |
| Hardcoded bundle id `com.ataca.image-browser` in release fallback | A future bundle-id rename | Two places to update: tauri.conf.json + this constant. There is no compile-time check that they match. |
| Per-root thumbnail dir creation is lazy | Calling `thumbnails_dir_for_root(99)` for a root that doesn't exist | Creates the subfolder anyway. Cleanup happens in `remove_root`; orphan subfolders for roots that were never used would persist. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **fsync-based atomic save** for settings.json. Today's `.tmp` + rename is good enough on every modern filesystem the app realistically runs on, but the trade-off is one syscall and could be added if a real corruption is observed.
- **Compile-time check that BUNDLE_ID matches tauri.conf.json**. Could be done via `build.rs` reading the conf and `concat!`-ing the const. Low priority.
- **Configurable Library/ location**. Today's dev path is hardcoded to `<repo>/Library/`. A power user might want to point at a network share. Not on the roadmap.
- **Cleanup on app uninstall**. If the user uninstalls a release build, the `~/Library/Application Support/...` directory persists (containing potentially many GBs of thumbnails and models). The README could document `rm -rf` instructions; the app itself doesn't surface a clean-up command.

## Durable Notes / Discarded Approaches

- **`Library/` chosen over `app_data/` or `.image-browser/`** for the dev directory because the user wanted it visible in the IDE file tree and clearly named (Library is recognisably "platform-data-shaped" on macOS). It's project-local in dev to make wiping state trivial (one `rm -rf`).
- **The file is gitignored** (`.gitignore` covers `Library/`, `*.onnx`, `*.db`, `*.db-journal`, `cosine_cache.bin`, `*.part`) so generated state never lands in commits even if the user accidentally `git add .`s.
- **Per-root thumbnail subfolders were a Phase 9 reorg driven by user feedback.** The pre-Phase-9 flat layout meant `remove_root` left orphaned thumbnail files on disk forever (the DB rows were CASCADE-deleted but the JPEG files weren't). Per-root subfolders make `rm -rf` the cleanup path; legacy NULL-root_id rows still write to the flat layout.
- **`cfg(debug_assertions)` is the dev/release switch, not a runtime env var.** Compile-time branching means the binary is hardcoded for one mode; you can't accidentally point a release binary at the project folder or vice versa.
- **`CARGO_MANIFEST_DIR` over a relative path** because Cargo guarantees its absolute resolution, while `./Library/` would depend on cwd at launch time.
- **`Cow::Borrowed` return for `strip_windows_extended_prefix`** is the audit-extraction's payoff: zero allocation on every non-Windows code path. The previous inline closure always returned `String::to_string()` even when no strip happened.
- **Settings.json is intentionally not a god-config**. User preferences belong in the frontend localStorage layer (`useUserPreferences`); the backend Settings struct is exclusively for state that needs to survive across migrations or be readable before the frontend is alive (the legacy scan_root migration is the only example).

## Obsolete / No Longer Relevant

The pre-Phase-3 layout where `images.db` lived next to `src-tauri/Cargo.toml` (i.e., `src-tauri/../images.db`) is gone. Everything is under the centralised `Library/` directory. The `Settings.scan_root` field is preserved for migration but never written by current code paths.
