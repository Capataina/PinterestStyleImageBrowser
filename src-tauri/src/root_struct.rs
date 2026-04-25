//! Root folder configured for indexing. Multi-folder support means a user
//! can point the app at any number of source directories and toggle
//! each on or off without losing the index.

use serde::{Deserialize, Serialize};

use crate::db::ID;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    pub id: ID,
    pub path: String,
    pub enabled: bool,
    /// Unix epoch seconds. Useful for "Recently added" sort order.
    pub added_at: i64,
}

impl Root {
    pub fn new(id: ID, path: String, enabled: bool, added_at: i64) -> Self {
        Self {
            id,
            path,
            enabled,
            added_at,
        }
    }
}
