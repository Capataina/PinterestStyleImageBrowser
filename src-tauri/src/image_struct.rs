use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{db::ID, tag_struct::Tag};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageData {
    pub id: ID,
    pub name: String,
    pub path: String,
    pub tags: Vec<Tag>,
    /// Path to the thumbnail image (smaller version for grid display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_path: Option<String>,
    /// Original image width in pixels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Original image height in pixels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl ImageData {
    pub fn new(id: ID, path: &Path, tags: Vec<Tag>) -> Self {
        // Try to canonicalize the path, but fall back to the original path if it doesn't exist
        // This handles cases where paths in the database point to files that have been moved/deleted
        let path_str = path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        ImageData {
            id,
            name,
            path: path_str,
            tags,
            thumbnail_path: None,
            width: None,
            height: None,
        }
    }

}
