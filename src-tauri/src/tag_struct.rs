use serde::{Deserialize, Serialize};

use crate::db::ID;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: ID,
    pub name: String,
    pub color: String,
}

impl Tag {
    pub fn new(id: ID, name: String, color: String) -> Self {
        return Self { id, name, color };
    }
}
