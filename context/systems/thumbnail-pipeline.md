# thumbnail-pipeline

*Maturity: working*

## Scope / Purpose

Generates and caches small JPEG thumbnails for every image in the database. Stores them on disk under `Library/thumbnails/root_<id>/thumb_<image_id>.jpg` (per-root subdirectory layout, Phase 9 reorg) and writes the thumbnail path plus original dimensions back to the `images` table for fast frontend layout. The grid does not load full-resolution images — only thumbnails. The full image is loaded only when the modal opens.

Runs in parallel via rayon during the indexing pipeline's Phase::Thumbnail. Single-SELECT path-to-root resolution (audit fix `0bdb5f4`) replaced the previous per-image DB query.

## Boundaries / Ownership

- **Owns:** thumbnail file naming, dimension math, format choice (JPEG), upscale prevention, per-root subdirectory layout.
- **Does not own:** the SQL update itself (delegates to `db.update_image_thumbnail`), full-resolution image rendering (frontend swaps to `props.item.url` only when `isSelected`), the rayon parallelisation (lives in the indexing pipeline), the per-root subfolder paths (delegates to `paths::thumbnails_dir_for_root(root_id)`).
- **Public API:** `ThumbnailGenerator::new(thumbnail_dir, max_width, max_height)`, `generate_thumbnail(image_path, image_id, root_id: Option<i64>)`, `get_thumbnail_path(image_id, root_id)`.

## Current Implemented Reality

### Per-root subfolder layout (Phase 9)

```
Library/thumbnails/
  root_1/thumb_42.jpg
  root_2/thumb_99.jpg
  root_3/thumb_12.jpg
  thumb_<id>.jpg                  ← legacy NULL-root_id rows go to the flat layout
```

Pre-Phase-9 was flat (`thumbnails/thumb_<id>.jpg`), which meant `remove_root` left orphaned files on disk forever. Per-root subfolders make `remove_root`'s cleanup a single `rm -rf` of the subfolder. Legacy un-migrated `root_id = NULL` rows continue writing to the flat layout via `paths::thumbnails_dir()` directly.

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

The indexing pipeline instantiates with `max_width=400, max_height=400`. Aspect ratio is preserved. Most thumbnails end up around 400×N or N×400.

### Generation

```rust
let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;
let (orig_w, orig_h) = (img.width(), img.height());
if thumbnail_path.exists() {                    // disk cache hit — no work
    return Ok(ThumbnailResult { thumbnail_path, original_width, original_height });
}
let (w, h) = compute_thumbnail_dimensions(orig_w, orig_h, max_width, max_height);
let thumb = img.thumbnail(w, h);                // image::thumbnail uses Lanczos3
thumb.save_with_format(&thumbnail_path, ImageFormat::Jpeg)?;
```

`thumbnail/generator.rs`. The disk-cache short-circuit (`if thumbnail_path.exists()`) makes the indexing pipeline's per-image work near-zero on re-runs against a populated thumbnails directory — only the DB update fires (and only for rows where `thumbnail_path` is still NULL or empty).

### Parallel execution

```rust
// indexing.rs::run_pipeline_inner Phase::Thumbnail
let path_to_root = database.get_paths_to_root_ids().unwrap_or_default();
needs_thumbs.par_iter().for_each(|image| {
    let root_id = path_to_root.get(&image.path).copied().flatten();
    match thumbnail_generator.generate_thumbnail(Path::new(&image.path), image.id, root_id) {
        Ok(result) => {
            if let Err(e) = database.update_image_thumbnail(image.id, &result.thumbnail_path, ...) {
                warn!("DB update for thumbnail of image {} failed: {e}", image.id);
            }
        }
        Err(e) => warn!("thumbnail generation failed for {}: {e}", image.path),
    }
    // emit progress event every ~25 thumbnails (atomic-bucket coalesced)
});
```

Per-image cost is dominated by JPEG decode + encode, which is embarrassingly parallel. The DB write under the mutex is microseconds vs ~100 ms decode/encode, so contention there is negligible. On an M-series chip with 8-12 cores this gives a ~6-10× speedup vs the previous serial loop.

### Single-SELECT path → root_id (audit fix)

```rust
let path_to_root = database.get_paths_to_root_ids().unwrap_or_default();
```

Replaces the previous N+1 pattern (`get_root_id_by_path` per image-needing-thumbnail held the DB Mutex 1500 times in rapid succession on a typical first run). The new `get_paths_to_root_ids` returns the entire (path, root_id) map in one query, matching the pattern `cosine.populate_from_db` already uses for embeddings.

`unwrap_or_default()` preserves the previous failure semantic: if the SELECT fails, downstream `generate_thumbnail` falls back to the legacy flat thumbnail directory (root_id None).

## Key Interfaces / Data Flow

```
indexing.rs::run_pipeline_inner Phase::Thumbnail:
  thumbnail_generator = ThumbnailGenerator::new(paths::thumbnails_dir(), 400, 400)?
  needs_thumbs        = db.get_images_without_thumbnails()?       (LEFT JOIN, returns ImageData)
  path_to_root        = db.get_paths_to_root_ids()?               (single SELECT)
  
  needs_thumbs.par_iter().for_each(|image| {
      root_id = path_to_root.get(&image.path).copied().flatten()
      result  = thumbnail_generator.generate_thumbnail(Path::new(&image.path), image.id, root_id)
      db.update_image_thumbnail(image.id, &result.thumbnail_path, w, h)
      throttled emit Phase::Thumbnail
  });
  emit Phase::Thumbnail final tick
```

Frontend later receives the thumbnail path via `ImageData::thumbnail_path` in `get_images_with_thumbnails`. `convertFileSrc(thumbnail_path)` produces a Tauri asset-protocol URL the WebView can load.

## Implemented Outputs / Artifacts

- One JPEG per image at `Library/thumbnails/root_<id>/thumb_<image_id>.jpg` (or flat for legacy NULL-root_id rows).
- DB row updated with `thumbnail_path`, `width`, `height` (the dimensions are the *original* image's, not the thumbnail's — used by Masonry for aspect-preserving layout).
- Tracing span `pipeline.thumbnail_phase` for perf attribution.
- Throttled `indexing-progress` events every ~25 thumbnails.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Thumbnail decode failure for a corrupt image | A bad `.jpg` byte | Logs warn; DB row stays unmarked; next pipeline run retries. Eventually the row stays orphan-thumbnail and the grid shows no tile (today the frontend falls back to a placeholder). |
| Per-root subfolder leaks if root removed without going through `remove_root` | A user manually deletes a roots row from the DB | The subfolder isn't `rm -rf`'d. Cosmetic only. |
| Disk-cache short-circuit assumes `thumbnail_path` matches the file on disk | A user manually deletes the JPEG file | The DB row points at a non-existent path; the WebView gets a 404 / empty asset. The next pipeline run wouldn't re-generate because the row still has `thumbnail_path NOT NULL`. Fix: `get_images_without_thumbnails` could verify `Path::new(&thumbnail_path).exists()` — adds N stat calls but is correct. |
| Rayon `par_iter().for_each(...)` panics propagate | A panic in `generate_thumbnail` | Rayon catches and re-throws on join; the indexing pipeline body would error out; `Phase::Error` emitted. Whole-pipeline failure for one bad image. |
| `get_paths_to_root_ids` returns the whole table | Very large libraries (100k+ images) | The HashMap grows to 100k entries; ~10 MB of paths held in memory during the thumbnail phase. Acceptable for the libraries the app realistically targets. |
| `image::thumbnail` defaults to a fast-but-not-best resize filter | Subtle quality differences vs Lanczos3 | Acceptable for thumbnail-quality previews. The CLIP encoder uses `FilterType::Nearest` (worse) — this isn't the bottleneck. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- **Verify thumbnail file existence in `get_images_without_thumbnails`** so manual JPEG deletion triggers regeneration.
- **WebP / AVIF output** for smaller files at the cost of decode speed in the WebView.
- **Per-image error recovery in the rayon loop** — wrap each iteration body in `catch_unwind` so one panic doesn't kill the whole pipeline.
- **Progressive JPEG output** so partial loads show a low-quality preview faster.

## Durable Notes / Discarded Approaches

- **400×400 max dimension** is a balance between visual quality on the grid (typical tile is ~250-400 px wide on a 1440p screen) and disk usage (a 400×400 JPEG is ~15-30 KB; 1500 thumbnails ≈ 30 MB).
- **JPEG over WebP** because every browser/WebView decodes JPEG quickly with no surprises, and the encoder is built into the `image` crate. WebP would save 20-30% of disk but add load overhead in some WebView versions.
- **Original dimensions stored in DB, not thumbnail dimensions.** Masonry layout uses original aspect ratio (so a 16:9 photo's tile is 16:9-shaped). Storing thumb dimensions would lose the aspect info.
- **Per-root subfolders are the Phase 9 reorg.** Pre-Phase-9 was flat; root removal left orphan files. Now `remove_root` `rm -rf`s the subfolder. Legacy NULL-root_id rows still write flat — the dual layout is intentional.
- **The disk-cache short-circuit assumes the file exists if the DB says so.** Trade-off: avoids a stat() per image on warm runs but trusts the DB-vs-disk consistency. The `Planned / Missing` item above proposes verifying.
- **Rayon over manual thread pools** because the parallelism is per-image (embarrassingly parallel, no inter-task communication) and rayon's work-stealing matches the workload shape perfectly.

## Obsolete / No Longer Relevant

The pre-Phase-5 sequential thumbnail loop is gone (now rayon-parallel). The pre-Phase-9 flat layout is gone (now per-root subfolders). The N+1 `get_root_id_by_path` is gone (replaced by `get_paths_to_root_ids`).
