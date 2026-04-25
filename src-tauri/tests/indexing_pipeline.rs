//! Integration test of the scan + thumbnail half of the indexing
//! pipeline.
//!
//! Encoder phase requires the ONNX model on disk, which we don't
//! ship with the repo, so this test stops short of embeddings. The
//! goal is to catch glue regressions between filesystem scanning,
//! the multi-folder roots table, the thumbnail generator, and the
//! orphan-detection pass — all the moving parts that broke in
//! recent sessions.

use image_browser_lib::db::ImageDatabase;
use image_browser_lib::filesystem::ImageScanner;
use image_browser_lib::thumbnail::ThumbnailGenerator;
use std::fs;
use std::path::PathBuf;

/// Write a small valid JPEG to disk. The image crate's encoder is the
/// quickest way to produce a real JPEG without bundling test fixtures
/// in the repo.
fn write_test_jpeg(path: &PathBuf, w: u32, h: u32) {
    let mut buf = image::RgbImage::new(w, h);
    // Fill with a gradient so the file isn't 0-byte and JPEG
    // compression gives it some heft.
    for (x, y, px) in buf.enumerate_pixels_mut() {
        *px = image::Rgb([
            ((x * 255) / w) as u8,
            ((y * 255) / h) as u8,
            128,
        ]);
    }
    image::DynamicImage::ImageRgb8(buf)
        .save_with_format(path, image::ImageFormat::Jpeg)
        .expect("write jpeg");
}

/// Helper: stand up a temp project layout with a roots dir + sample
/// images, ready for the pipeline functions to chew on.
fn setup_workspace() -> (tempfile::TempDir, ImageDatabase, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root_path = tmp.path().join("photos");
    fs::create_dir(&root_path).unwrap();
    // 3 JPEGs at the top, 1 in a subfolder (recursive scan check).
    write_test_jpeg(&root_path.join("a.jpg"), 200, 100);
    write_test_jpeg(&root_path.join("b.jpg"), 100, 200);
    write_test_jpeg(&root_path.join("c.jpg"), 150, 150);
    let sub = root_path.join("sub");
    fs::create_dir(&sub).unwrap();
    write_test_jpeg(&sub.join("d.jpg"), 300, 100);

    let db_path = tmp.path().join("test.db");
    let thumb_dir = tmp.path().join("thumbnails");
    fs::create_dir(&thumb_dir).unwrap();

    let db = ImageDatabase::new(db_path.to_str().unwrap()).unwrap();
    db.initialize().unwrap();

    (tmp, db, root_path, thumb_dir)
}

#[test]
fn scan_inserts_every_image_under_root() {
    let (_tmp, db, root_path, _thumb_dir) = setup_workspace();
    let scanner = ImageScanner::new();
    let paths = scanner.scan_directory(&root_path).unwrap();
    // 3 top-level + 1 nested = 4 total
    assert_eq!(paths.len(), 4);

    // Add a root and insert each. Should idempotently land 4 rows.
    let root = db.add_root(root_path.to_string_lossy().into_owned()).unwrap();
    for p in &paths {
        db.add_image(p.clone(), Some(root.id)).unwrap();
    }
    let imgs = db.get_all_images().unwrap();
    assert_eq!(imgs.len(), 4);
}

#[test]
fn scan_is_idempotent_on_re_run() {
    let (_tmp, db, root_path, _thumb_dir) = setup_workspace();
    let scanner = ImageScanner::new();
    let paths = scanner.scan_directory(&root_path).unwrap();
    let root = db.add_root(root_path.to_string_lossy().into_owned()).unwrap();
    for p in &paths {
        db.add_image(p.clone(), Some(root.id)).unwrap();
    }
    // Second run: same paths, no new rows.
    for p in &paths {
        db.add_image(p.clone(), Some(root.id)).unwrap();
    }
    let imgs = db.get_all_images().unwrap();
    assert_eq!(imgs.len(), 4);
}

#[test]
fn thumbnail_generator_produces_files_under_per_root_subdir() {
    let (_tmp, db, root_path, thumb_dir) = setup_workspace();
    let scanner = ImageScanner::new();
    let paths = scanner.scan_directory(&root_path).unwrap();
    let root = db.add_root(root_path.to_string_lossy().into_owned()).unwrap();
    for p in &paths {
        db.add_image(p.clone(), Some(root.id)).unwrap();
    }

    // Generate thumbnails for each image into root_<id>/ subfolder.
    let generator = ThumbnailGenerator::new(&thumb_dir, 128, 128).unwrap();
    let needs = db.get_images_without_thumbnails().unwrap();
    assert_eq!(needs.len(), 4);
    for image in &needs {
        let result = generator
            .generate_thumbnail(
                std::path::Path::new(&image.path),
                image.id,
                Some(root.id),
            )
            .unwrap();
        // Thumbnail file actually landed on disk.
        assert!(result.thumbnail_path.exists());
        // And it's inside the per-root subfolder.
        assert!(
            result.thumbnail_path.to_string_lossy().contains("root_"),
            "thumbnail should be under a per-root subdir, got {}",
            result.thumbnail_path.display()
        );
        db.update_image_thumbnail(
            image.id,
            &result.thumbnail_path,
            result.original_width,
            result.original_height,
        )
        .unwrap();
    }

    // After processing: nothing left without a thumbnail.
    let remaining = db.get_images_without_thumbnails().unwrap();
    assert!(remaining.is_empty());
}

#[test]
fn orphan_detection_marks_disappeared_files() {
    let (_tmp, db, root_path, _thumb_dir) = setup_workspace();
    let scanner = ImageScanner::new();
    let paths = scanner.scan_directory(&root_path).unwrap();
    let root = db.add_root(root_path.to_string_lossy().into_owned()).unwrap();
    for p in &paths {
        db.add_image(p.clone(), Some(root.id)).unwrap();
    }

    // Delete one file from disk.
    fs::remove_file(root_path.join("a.jpg")).unwrap();

    // Re-scan — the alive set is now 3 files.
    let alive = scanner.scan_directory(&root_path).unwrap();
    assert_eq!(alive.len(), 3);
    db.mark_orphaned(root.id, &alive).unwrap();

    // get_images_with_thumbnails filters out orphaned rows.
    let visible = db
        .get_images_with_thumbnails(vec![], "".into(), false)
        .unwrap();
    assert_eq!(visible.len(), 3);
    let visible_paths: Vec<&str> =
        visible.iter().map(|i| i.path.as_str()).collect();
    assert!(!visible_paths.iter().any(|p| p.ends_with("a.jpg")));
}

#[test]
fn remove_root_cascade_takes_thumbnails_with_it_logically() {
    // We can't directly test the disk-side rm -rf inside this
    // integration test (it lives in the Tauri command, not the lib),
    // but we can verify the DB cascade: removing a root drops its
    // images, which means subsequent thumbnail-needs returns nothing.
    let (_tmp, db, root_path, _thumb_dir) = setup_workspace();
    let scanner = ImageScanner::new();
    let paths = scanner.scan_directory(&root_path).unwrap();
    let root = db.add_root(root_path.to_string_lossy().into_owned()).unwrap();
    for p in &paths {
        db.add_image(p.clone(), Some(root.id)).unwrap();
    }
    assert_eq!(db.get_all_images().unwrap().len(), 4);

    db.remove_root(root.id).unwrap();
    assert!(db.get_all_images().unwrap().is_empty());
    assert!(db.get_images_without_thumbnails().unwrap().is_empty());
}

#[test]
fn multi_folder_grid_query_unions_enabled_roots() {
    let tmp = tempfile::tempdir().unwrap();

    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();
    write_test_jpeg(&dir_a.join("a1.jpg"), 64, 64);
    write_test_jpeg(&dir_a.join("a2.jpg"), 64, 64);
    write_test_jpeg(&dir_b.join("b1.jpg"), 64, 64);

    let db_path = tmp.path().join("multi.db");
    let db = ImageDatabase::new(db_path.to_str().unwrap()).unwrap();
    db.initialize().unwrap();

    let root_a = db.add_root(dir_a.to_string_lossy().into_owned()).unwrap();
    let root_b = db.add_root(dir_b.to_string_lossy().into_owned()).unwrap();
    let scanner = ImageScanner::new();
    for p in scanner.scan_directory(&dir_a).unwrap() {
        db.add_image(p, Some(root_a.id)).unwrap();
    }
    for p in scanner.scan_directory(&dir_b).unwrap() {
        db.add_image(p, Some(root_b.id)).unwrap();
    }

    // Both enabled → union of 3 images.
    let imgs = db
        .get_images_with_thumbnails(vec![], "".into(), false)
        .unwrap();
    assert_eq!(imgs.len(), 3);

    // Disable root_a → only b1.jpg visible.
    db.set_root_enabled(root_a.id, false).unwrap();
    let imgs = db
        .get_images_with_thumbnails(vec![], "".into(), false)
        .unwrap();
    assert_eq!(imgs.len(), 1);
    assert!(imgs[0].path.ends_with("b1.jpg"));

    // Re-enable, remove root_b → 2 images visible (only root_a's).
    db.set_root_enabled(root_a.id, true).unwrap();
    db.remove_root(root_b.id).unwrap();
    let imgs = db
        .get_images_with_thumbnails(vec![], "".into(), false)
        .unwrap();
    assert_eq!(imgs.len(), 2);
}
