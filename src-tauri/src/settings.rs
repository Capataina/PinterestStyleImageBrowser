//! User-facing settings persisted to `settings.json` in the app data dir.
//!
//! Currently very small (just the scan root). Pass 4 will populate the
//! `scan_root` field on first folder-pick. Future settings (per-tag
//! colours, thumbnail size, semantic-search defaults) extend this struct.
//!
//! The file may not exist on first launch; callers treat missing-file
//! as "use defaults."

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::paths;

/// Persisted user preferences.
///
/// Every field is optional so older settings.json files (missing newly-
/// added fields) deserialise cleanly via serde defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Absolute path to the directory the app should index. None means
    /// no folder has been picked yet — first-launch state.
    #[serde(default)]
    pub scan_root: Option<PathBuf>,
}

impl Settings {
    /// Load settings from disk. Returns Settings::default() if the file
    /// does not exist or cannot be parsed (corrupt file = treat as
    /// fresh; we don't want a single bad byte to brick the app).
    pub fn load() -> Self {
        let path = paths::settings_path();
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Settings>(&content) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[settings] failed to parse {} ({e}); using defaults",
                        path.display()
                    );
                    Settings::default()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                eprintln!(
                    "[settings] failed to read {} ({e}); using defaults",
                    path.display()
                );
                Settings::default()
            }
        }
    }

    /// Persist settings to disk atomically (write to .tmp then rename).
    pub fn save(&self) -> io::Result<()> {
        let path = paths::settings_path();
        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&tmp, content)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_no_scan_root() {
        let s = Settings::default();
        assert!(s.scan_root.is_none());
    }

    #[test]
    fn test_round_trip_with_scan_root() {
        let s = Settings {
            scan_root: Some(PathBuf::from("/tmp/example")),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scan_root, Some(PathBuf::from("/tmp/example")));
    }

    #[test]
    fn test_unknown_fields_dont_break_parse() {
        // Future-proofing: an older binary reading a newer settings.json
        // (with extra fields) should still parse the bits it understands.
        let json = r#"{"scan_root": "/tmp/x", "future_field": "ignored"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.scan_root, Some(PathBuf::from("/tmp/x")));
    }

    #[test]
    fn test_missing_field_uses_default() {
        let json = r#"{}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert!(s.scan_root.is_none());
    }
}
