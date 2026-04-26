# paths-and-state

*Maturity: working*

## Scope / Purpose

Single source of truth for every disk path the app uses. Centralises the dev-vs-release branching, ensures every state file ends up under one logical root, and provides the small handful of helpers (`paths::*_dir()`) that every other system reads. Also owns the legacy `Settings` struct that records the pre-multi-folder `scan_root` field for the one-shot migration path.

This is a small but load-bearing module: every system that reads or writes state goes through here. A bug in `paths::app_data_dir()` would put state files in the wrong place silently.

## Boundaries / Ownership

- **Owns:** `paths::app_data_dir()` (the dev/release branching), `paths::database_path()`, `paths::thumbnails_dir()`, `paths::thumbnails_dir_for_root(id)`, `paths::models_dir()`, `paths::settings_path()`, `paths::cosine_cache_path()`, `paths::exports_dir()`, `paths::strip_windows_extended_prefix(&str) -> Cow<'_, str>`, the `Settings { scan_root: Option<PathBuf> }` struct + its load/save methods.
- **Does not own:** the file contents themselves (each owning system writes its own format), `Library/` itself (the directory is created on first call to `app_data_dir`), the bundle-id (just stores a constant for the release fallback).
- **Public API (paths):** see Owns above.
- **Public API (settings):** `Settings::default()`, `Settings::load() -> Self`, `Settings::save(&self) -> io::Result<()>`.

## Current Implemented Reality

### Library/ layout

```
<repo>/Library/                       # in dev (debug_assertions)
  images.db                           # SQLite (WAL adds .db-wal + .db-shm)
  settings.json                       # legacy single-folder scan_root field (one-shot migration)
  cosine_cache.bin                    # bincode-encoded Vec<(PathBuf, Vec<f32>)>
  models/
    model_image.onnx                  # downloaded from HuggingFace
    model_text.onnx                   # downloaded from HuggingFace
    tokenizer.json                    # downloaded from HuggingFace
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

In release builds, `app_data_dir()` returns `dirs::data_dir()/com.ataca.image-browser/` — on macOS that's `~/Library/Application Support/com.ataca.image-browser/`. The structure under it is identical.

### Dev vs release branching

```rust
pub fn app_data_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");  // src-tauri/
        let repo_root = std::path::Path::new(manifest_dir).parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = repo_root.join("Library");
        let _ = ensure_dir(&dir);
        return dir;
    }

    #[cfg(not(debug_assertions))]
    {
        let base = dirs::data_dir().unwrap_or_else(|| {
            warn!("dirs::data_dir() returned None; falling back to ./app-data");
            PathBuf::from("./app-data")
        });
        let dir = base.join(BUNDLE_ID);
        let _ = ensure_dir(&dir);
        dir
    }
}
```

`CARGO_MANIFEST_DIR` is captured at compile time by the `env!` macro and points at `src-tauri/` (where `Cargo.toml` lives). Stepping up one directory lands at the repo root, where `Library/` lives.

The dev path is project-local because the user wants `Library/` visible alongside the rest of the repo (easy to inspect, easy to wipe — `rm -rf Library` resets everything; trivial to share a debugging snapshot). The release path falls back to the platform's app-data directory because there is no project folder once the binary is bundled and shipped.

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
    if path_str.starts_with("\\\\?\\") {
        Cow::Owned(path_str[4..].to_string())
    } else {
        Cow::Borrowed(path_str)
    }
}
```

Used by `commands::resolve_image_id_for_cosine_path` to map cosine-result paths back to DB ids when the canonical form drifts (Windows-extended-prefix vs not). The `Cow` return means non-Windows paths pay zero allocation for the common case where the prefix isn't present.

This was extracted from a previously-triplicated inline closure (audit Pattern Extraction finding). The previous notes warned "don't add a fourth normalisation closure" — that warning is satisfied: the only normalisation site is now this helper.

### Legacy `Settings` struct

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub scan_root: Option<PathBuf>,
}
```

The struct is intentionally minimal. The `#[serde(default)]` attribute means newer binaries reading an older `settings.json` (without the field) deserialise cleanly to `None`; older binaries reading a newer `settings.json` (with extra fields) just ignore the unknown ones. Persisted atomically via write-to-`.tmp` + rename.

The `scan_root` field exists exclusively for the one-shot legacy migration path in `lib.rs::run::setup`. Once the migration runs, the field is cleared so it doesn't re-trigger on next launch.

User preferences (theme, columns, sortMode, etc.) live in the **frontend** localStorage, not in this struct — the `useUserPreferences` hook owns them. There is no current need for them to round-trip the backend.

## Key Interfaces / Data Flow

### Read sites

| Caller | Function | Purpose |
|--------|----------|---------|
| `db::ImageDatabase::default_database_path` | `database_path()` | Open the SQLite file |
| `lib.rs::run::setup` | `Settings::load()` | Check for legacy scan_root |
| `lib.rs::run::setup` | `Settings::save()` | Clear scan_root after legacy migration |
| `indexing.rs::run_pipeline_inner` | `models_dir().join(...)` | Verify model files exist |
| `indexing.rs::run_pipeline_inner` | `thumbnails_dir()` | Pass to ThumbnailGenerator::new |
| `indexing.rs::run_pipeline_inner` | `cosine_cache_path()` (indirectly via `cosine.save_to_disk`) | Persist cosine cache |
| `model_download.rs::download_models_if_missing` | `models_dir()` | Where to write downloads |
| `commands::semantic::semantic_search` | `models_dir()` | Lazy-init text encoder |
| `commands::roots::remove_root` | `thumbnails_dir_for_root(id)` | rm -rf the per-root subfolder |
| `commands::profiling::export_perf_snapshot` | `exports_dir()` | Write one-off perf snapshots |
| `commands::resolve_image_id_for_cosine_path` | `strip_windows_extended_prefix(...)` | Path lookup fallback |
| `main.rs` | `paths::exports_dir()` (via `perf::init_session`) | Initialize profiling session dir |

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
