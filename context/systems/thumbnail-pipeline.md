# thumbnail-pipeline

*Maturity: working*

## Scope / Purpose

Generates and caches small JPEG thumbnails for every image in the database. Stores them on disk as `.thumbnails/thumb_{id}.jpg` and writes the thumbnail path plus original dimensions back to the `images` table for fast frontend layout. The grid does not load full-resolution images — only thumbnails.

## Boundaries / Ownership

- **Owns:** thumbnail file naming, dimension math, format choice (JPEG), upscale prevention.
- **Does not own:** the SQL update itself (delegates to `database`), full-resolution image rendering (frontend swaps to `props.item.url` only when `isSelected`).
- **Public API:** `ThumbnailGenerator::new(thumbnail_dir, max_width, max_height)`, `generate_thumbnail(image_path, image_id)`, `generate_all_missing_thumbnails(&db)`, `get_thumbnail_path(image_id)`.

## Current Implemented Reality

### Sizing math

```rust
let width_ratio  = max_width  as f32 / width  as f32;
let height_ratio = max_height as f32 / height as f32;
let ratio        = width_ratio.min(height_ratio).min(1.0); // do not upscale
let new_width    = (width  as f32 * ratio).round() as u32;
let new_height   = (height as f32 * ratio).round() as u32;
(new_width.max(1), new_height.max(1))
```

Source: `thumbnail/generator.rs:97-106`. The `min(1.0)` clamp prevents upscaling smaller images. The `.max(1)` floor prevents division-by-zero artefacts on tiny inputs.

### Thumbnail dimensions

`main.rs:36-38` instantiates with `max_width=400, max_height=400`. Aspect ratio is preserved.

### Generation

```rust
let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;
let (orig_w, orig_h) = (img.width(), img.height());
if thumbnail_path.exists() {                    // disk cache hit — no work
    return Ok(ThumbnailResult { thumbnail_path, original_width, original_height });
}
let (tw, th) = self.calculate_thumbnail_size(orig_w, orig_h);
let thumbnail = img.thumbnail(tw, th);          // image crate uses Lanczos3 internally
thumbnail.save_with_format(&thumbnail_path, image::ImageFormat::Jpeg)?;
```

Source: `thumbnail/generator.rs:55-93`. Note that `image::DynamicImage::thumbnail()` uses Lanczos3 — this is *not* the same as the CLIP preprocessor's `FilterType::Nearest` (see `clip-image-encoder.md`).

### Batch generation

`generate_all_missing_thumbnails` queries `db.get_images_without_thumbnails()`, then loops sequentially. Per image: generate file, call `db.update_image_thumbnail(id, path, w, h)`. Logs progress every 10 images.

The loop is **single-threaded**. For 749 images at a few hundred milliseconds each, the initial pass is several minutes.

## Key Interfaces / Data Flow

```text
main.rs (startup)
    ──► ThumbnailGenerator::new(.thumbnails/, 400, 400)
    ──► generate_all_missing_thumbnails(&database)
        ──► db.get_images_without_thumbnails()
        ──► for each image:
              generate_thumbnail(path, id)
                ──► open + decode (one decode per image; full image → memory)
                ──► if file exists: return early (don't re-encode)
                ──► else: img.thumbnail(...) + save JPEG
              db.update_image_thumbnail(id, path, w, h)
```

Output filenames follow `thumb_{id}.jpg` exactly (`generator.rs:51`). The frontend (`services/images.ts:78-80`) reconstructs the same pattern when it needs a thumbnail URL but the backend did not return one.

## Implemented Outputs / Artifacts

- `.thumbnails/thumb_{id}.jpg` files on disk (the directory is created with `fs::create_dir_all` at `ThumbnailGenerator::new`).
- DB columns `thumbnail_path`, `width`, `height` populated.
- Frontend `convertFileSrc(thumbnail_path)` produces a `tauri://localhost/...`-style URL the WebView can render directly.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Serial generation | Any first-launch on a large library | Several minutes of single-threaded JPEG encoding before the UI is usable. `rayon::par_iter` would be ~4-8× faster on multi-core. |
| Disk-cache + DB-row drift | `update_image_thumbnail` fails after the file is written | Next run, `get_images_without_thumbnails` finds the row again, but `if thumbnail_path.exists()` short-circuits the decode/encode. So the regeneration cost is just a `stat()` — not catastrophic, but the DB still gets re-written each launch until the call succeeds. |
| `.thumbnails/` is repo-relative | Running the app from a different working directory | The thumbnail directory is created where `cargo tauri dev` is run from. A different `cwd` produces a different `.thumbnails/`, and the DB rows still point at the old paths. |
| Fixed 400×400 cap | A 4K display showing 4-column grid | Each tile may be 600+ pixels wide; thumbnails get scaled up by the browser, looking soft. The cap was chosen for the original 2-column grid scale. |
| No re-thumbnail on size config change | Changing `max_width/max_height` in `main.rs` | Existing rows keep their old thumbnail dims forever — the cache existence check short-circuits. Only newly-added images get the new size. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Parallelise via `rayon` over the iter, sharing the DB Mutex.
- Configurable target dimensions (likely surfaced in the eventual settings UI).
- Cache invalidation when scan root changes — today there is none.

## Durable Notes / Discarded Approaches

- The thumbnail size of 400×400 is hardcoded at the call site (`main.rs:36-38`), not in the generator. This was intentional so a future settings UI could tune it without touching the module.
- `image::DynamicImage::thumbnail()` and `image::imageops::FilterType::Nearest` (in `encoder.rs`) are different code paths. The thumbnail pipeline correctly uses Lanczos3 for visual quality; the CLIP preprocessor uses Nearest for ML preprocessing — that latter choice is a quality concern flagged separately in `notes/clip-preprocessing-decisions.md`.
- The cache-check `if thumbnail_path.exists()` at `generator.rs:61-67` is the reason a rerun is fast. Without it, every launch would re-encode every JPEG. The trade-off is: the cache cannot detect file corruption — once the file exists, it is trusted.

## Obsolete / No Longer Relevant

None.
