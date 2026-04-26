use std::sync::{Arc, Mutex};

use image_browser_lib::db::ImageDatabase;
use image_browser_lib::similarity_and_semantic_search::cosine::CosineIndex;
use image_browser_lib::CosineIndexState;

fn fresh_db() -> (tempfile::TempDir, ImageDatabase) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("images.db");
    let db = ImageDatabase::new(db_path.to_str().unwrap()).unwrap();
    db.initialize().unwrap();
    (dir, db)
}

#[test]
#[ignore = "diagnostic currently exposes cache invalidation bug after root commands clear cached_images without clearing current_encoder_id"]
fn ensure_loaded_for_repopulates_when_current_encoder_matches_but_cache_is_empty() {
    let (_dir, db) = fresh_db();
    db.add_image("/a.jpg".to_string(), None).unwrap();
    db.add_image("/b.jpg".to_string(), None).unwrap();
    let id_a = db.get_image_id_by_path("/a.jpg").unwrap();
    let id_b = db.get_image_id_by_path("/b.jpg").unwrap();
    db.update_image_embedding(id_a, vec![1.0, 0.0, 0.0]).unwrap();
    db.update_image_embedding(id_b, vec![0.0, 1.0, 0.0]).unwrap();

    let state = CosineIndexState {
        index: Arc::new(Mutex::new(CosineIndex::new())),
        db_path: String::new(),
        current_encoder_id: Arc::new(Mutex::new(String::new())),
    };

    state.ensure_loaded_for(&db, "clip_vit_b_32").unwrap();
    assert_eq!(state.index.lock().unwrap().cached_images.len(), 2);

    state.index.lock().unwrap().cached_images.clear();
    assert_eq!(
        state.current_encoder_id.lock().unwrap().as_str(),
        "clip_vit_b_32",
        "root commands currently preserve this marker while clearing the cache",
    );

    state.ensure_loaded_for(&db, "clip_vit_b_32").unwrap();
    assert_eq!(
        state.index.lock().unwrap().cached_images.len(),
        2,
        "ensure_loaded_for must repopulate an empty cache even when the encoder id marker already matches",
    );
}
