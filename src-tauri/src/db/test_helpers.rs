//! Shared `fresh_db()` helper for `db/*` submodule tests.
//!
//! Lifted out of the original monolithic `db.rs` test module so the
//! split submodules can each `use super::test_helpers::fresh_db;`
//! without duplicating the constructor across files.

use super::ImageDatabase;

pub(super) fn fresh_db() -> ImageDatabase {
    let db = ImageDatabase::new(":memory:").unwrap();
    db.initialize().unwrap();
    db
}
