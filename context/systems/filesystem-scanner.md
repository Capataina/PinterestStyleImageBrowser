# filesystem-scanner

*Maturity: working*

## Scope / Purpose

Recursively walks a root directory, filters by image extension, and returns a flat `Vec<String>` of paths. Used exactly once at startup, against a hardcoded path. No watcher, no rescan, no concurrency.

## Boundaries / Ownership

- **Owns:** the recursion, the extension whitelist, the path-string conversion.
- **Does not own:** writing to the database (`main::index_directory` does that), file-existence validation beyond `is_file()`/`is_dir()`.
- **Public API:** `ImageScanner::new() -> Self`, `scan_directory(&Path) -> Result<Vec<String>, std::io::Error>`.

## Current Implemented Reality

### Algorithm

```text
fn scan_directory(root):
    for entry in std::fs::read_dir(root):
        if entry.is_dir():
            recurse → append nested paths
        elif entry.is_file() and is_supported_image(path):
            push path.to_string_lossy().to_string()
    return paths
```

Source: `filesystem.rs:22-41`.

### Extension whitelist

```rust
const SUPPORTED_IMAGE_EXTENSIONS: [&str; 7] =
    ["jpg", "png", "gif", "jpeg", "bmp", "tiff", "webp"];
```

Comparison is case-insensitive — extension is `.to_lowercase()`-d before `contains`. (`filesystem.rs:3-12`.)

### Where it runs

`main.rs:24` instantiates `ImageScanner` and feeds the result to `db.add_image` per path. The root is hardcoded as `Path::new("test_images")`. There is no folder picker.

## Key Interfaces / Data Flow

```text
main.rs::index_directory(test_path, &mut database)
    ──► ImageScanner::scan_directory(root)  → Result<Vec<String>>
        ──► std::fs::read_dir (synchronous, blocking)
        ──► std::fs::DirEntry::file_type     (one syscall per entry)
    ──► for path in result: db.add_image(path).unwrap()
```

The pipeline is single-threaded. For 749 images the cost is dominated by the per-entry stat() calls; the encoder pass downstream is the actual bottleneck.

## Implemented Outputs / Artifacts

- `Vec<String>` of absolute paths suitable for `INSERT INTO images(path) VALUES(?)`.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Hardcoded scan root (`test_images`) | First-time user trying their own library | App is unusable against a user's own folder without rebuilding from source. README claims a folder picker exists. |
| Stale unit test `test_scan_directory_finds_all_images` | Running `cargo test` | Test asserts `results.len() == 4` against a folder with 749 images. Test fails red. |
| Symlinks, junctions, and `.thumbnails/` self-recursion | Putting `.thumbnails/` underneath the scan root (currently it lives at repo root, but a future move could change this) | `.thumbnails/thumb_42.jpg` would itself be picked up as a `.jpg`. The current code has no exclusion list. |
| Permission errors propagate via `?` | A subdirectory the user cannot read | Whole scan aborts with `Err(io::Error)`; `main::index_directory` only `eprintln!`s and continues with whatever was scanned successfully *before* the error — no, actually it does **not** continue. It bails on the first error because `scan_directory` returns `Err` and `main::index_directory` matches against `Err(e)` with only an eprintln. The DB is then left with whatever was inserted before the loop started, which is nothing (the loop runs over the OK case only). |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- User-selectable folder via Tauri `dialog` plugin + a `scan_directory(path)` Tauri command. This is the single highest-leverage change and is documented as a Tier 1 priority across the project's README/Gaps notes.
- Runtime rescan command for adding new images without restarting.
- Exclude the `.thumbnails/` directory explicitly to be safe against future scan-root shifts.
- Consider `walkdir` for Windows symlink handling and consistent relative paths. The author left a `// CAN USE WALKDIR` comment at `filesystem.rs:21` documenting that this was considered.

## Durable Notes / Discarded Approaches

- Author considered `walkdir` (comment at `filesystem.rs:21`) but went with `std::fs::read_dir` recursion to avoid the dependency. For a 749-image case this is fine; for symlink-heavy or tens-of-thousands cases `walkdir` would be more robust.
- `scan_directory` returns paths via `path.to_string_lossy().to_string()`. Lossy because Windows paths can contain non-UTF-8 codepoints — but in practice every image path the app sees is UTF-8-clean, and `to_string_lossy()` is safer than `to_str().unwrap()`.

## Obsolete / No Longer Relevant

None.
