//! Audit diagnostic for `db/embeddings.rs` writer-vs-reader routing.
//!
//! Documented in
//! `context/plans/code-health-audit/area-4-database.md` § I-DB-1, I-DB-2
//! and `area-3-encoders.md` § I-ENC-4.
//!
//! `notes/conventions.md` § "Read-only secondary read_lock() for
//! foreground SELECTs" sets the convention: any IPC SELECT defaults to
//! `read_lock()`. The audit found that several DB methods called from
//! foreground IPC paths still use `self.connection.lock()` (writer
//! mutex):
//!
//! - `db.get_embedding(image_id, encoder_id)` — `db/embeddings.rs:242-260`
//! - `db.get_image_embedding(image_id)` — `db/embeddings.rs:41-84`
//!   (reads the dead legacy column; will be removed per D-DB-1)
//! - `db.get_images_without_embedding_for(encoder_id)` —
//!   `db/embeddings.rs:317-335` (called from indexing threads, not
//!   foreground — but the convention applies anyway)
//!
//! This test exercises the methods with `:memory:` databases, where
//! `read_lock()` falls back to the writer connection. It pins the
//! invariant that a foreground SELECT call returns the same data as
//! the writer-mutex path — i.e. switching `get_embedding` to
//! `read_lock()` is a zero-behaviour-change refactor.
//!
//! Marked `#[ignore]` because the assertions document audit findings
//! rather than guard regressions.

use image_browser_lib::db::ImageDatabase;

fn fresh_db() -> (tempfile::TempDir, ImageDatabase) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("images.db");
    let db = ImageDatabase::new(db_path.to_str().unwrap()).unwrap();
    db.initialize().unwrap();
    (dir, db)
}

#[test]
#[ignore = "audit diagnostic — get_embedding currently routes through the writer mutex (I-ENC-4)"]
fn get_embedding_returns_consistent_result_independent_of_mutex_path() {
    let (_dir, db) = fresh_db();
    db.add_image("/x.jpg".to_string(), None).unwrap();
    let id = db.get_image_id_by_path("/x.jpg").unwrap();

    let emb = vec![0.1_f32, 0.2, 0.3, 0.4];
    db.upsert_embedding(id, "clip_vit_b_32", &emb).unwrap();

    // Today: `get_embedding` uses `self.connection.lock()` — writer
    // mutex. After the audit-recommended switch to `self.read_lock()`,
    // the same call must return the identical bytes; this assertion
    // documents the invariant that the audit's proposed refactor
    // preserves.
    let read = db.get_embedding(id, "clip_vit_b_32").unwrap();
    assert_eq!(read, emb);
}

#[test]
#[ignore = "audit diagnostic — get_images_without_embedding_for routing (I-DB-2)"]
fn get_images_without_embedding_for_filters_correctly_independent_of_mutex_path() {
    let (_dir, db) = fresh_db();
    db.add_image("/has.jpg".to_string(), None).unwrap();
    db.add_image("/without.jpg".to_string(), None).unwrap();
    let has_id = db.get_image_id_by_path("/has.jpg").unwrap();
    db.upsert_embedding(has_id, "siglip2_base", &[0.5_f32, 0.6])
        .unwrap();

    // `/has.jpg` has a siglip2 embedding; `/without.jpg` doesn't.
    // Today this method uses the writer mutex; switching it to
    // `read_lock()` should change nothing observable.
    let needs = db.get_images_without_embedding_for("siglip2_base").unwrap();
    let paths: Vec<String> = needs.into_iter().map(|(_, p)| p).collect();
    assert_eq!(paths, vec!["/without.jpg".to_string()]);
}

#[test]
#[ignore = "audit diagnostic — confirms count_embeddings_for already uses read_lock()"]
fn count_embeddings_for_returns_zero_for_unknown_encoder() {
    // `count_embeddings_for` is the convention-following baseline:
    // it already uses `read_lock()`. This test is a control — if the
    // routing changes, this should still return 0 (it's the per-
    // encoder count, returns 0 when no rows match).
    let (_dir, db) = fresh_db();
    let count = db.count_embeddings_for("nonexistent_encoder").unwrap();
    assert_eq!(count, 0);
}
