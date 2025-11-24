use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{db::ID, tag_struct::Tag};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageData {
    pub id: ID,
    pub name: String,
    pub path: String,
    pub tags: Vec<Tag>,
}

impl ImageData {
    pub fn new(id: ID, path: &Path, tags: Vec<Tag>) -> Self {
        println!("{}", path.to_str().unwrap());
        let path_str = path
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap_or_default()
            .to_string();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();

        return ImageData {
            id,
            name,
            path: path_str,
            tags,
        };
    }
}
