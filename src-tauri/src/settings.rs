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
use tracing::error;

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

    /// LEGACY (deprecated 2026-04-26 with Phase 11c).
    ///
    /// Was the user's "primary" image encoder back when the picker was
    /// a single-choice dropdown. The Phase 5 RRF fusion + Phase 11c
    /// per-encoder toggles obsolete the priority concept: when fusion
    /// uses every enabled encoder, "which one runs first" no longer
    /// makes user-visible sense.
    ///
    /// Field kept on the struct so old settings.json files deserialise
    /// without erroring, but the value is ignored — the indexing
    /// pipeline reads `enabled_encoders` instead. Will be removed once
    /// every install has been bumped past pipeline-version 4.
    #[serde(default)]
    pub priority_image_encoder: Option<String>,

    /// Encoder IDs that are currently enabled. The indexing pipeline
    /// only encodes for encoders in this list; image-image fusion and
    /// text-image fusion only fuse over enabled encoders.
    ///
    /// `None` means "use the default set" (every supported encoder
    /// enabled), which gives a sane out-of-box experience for fresh
    /// installs and for users who had `priority_image_encoder` set
    /// under the legacy schema.
    ///
    /// User mutates via the `set_enabled_encoders` Tauri command from
    /// the Settings drawer's encoder toggles. Re-enabling an encoder
    /// that was previously enabled is INSTANT (its embeddings are still
    /// in the per-encoder embeddings table) — fusion picks it back up
    /// as soon as the next call lands. Enabling an encoder that has
    /// never run requires the next indexing pipeline pass to fill in
    /// its embeddings.
    #[serde(default)]
    pub enabled_encoders: Option<Vec<String>>,
}

/// Default encoder set when `enabled_encoders` is `None` — every
/// supported encoder enabled. Adding a 4th encoder = appending its id
/// here.
pub const DEFAULT_ENABLED_ENCODERS: &[&str] =
    &["clip_vit_b_32", "siglip2_base", "dinov2_base"];

impl Settings {
    /// Returns the resolved enabled-encoder list. Falls back to
    /// `DEFAULT_ENABLED_ENCODERS` when the user hasn't set a preference.
    /// Filters out empty strings so a corrupt settings.json with
    /// `"enabled_encoders": [""]` doesn't silently disable everything.
    pub fn resolved_enabled_encoders(&self) -> Vec<String> {
        match &self.enabled_encoders {
            Some(list) if !list.is_empty() => list
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect(),
            _ => DEFAULT_ENABLED_ENCODERS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }
}

impl Settings {
    /// (Inherent impls split — `resolved_enabled_encoders` lives above.)
    /// Load settings from disk. Returns Settings::default() if the file
    /// does not exist or cannot be parsed (corrupt file = treat as
    /// fresh; we don't want a single bad byte to brick the app).
    pub fn load() -> Self {
        let path = paths::settings_path();
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Settings>(&content) {
                Ok(s) => s,
                Err(e) => {
                    error!(
                        "failed to parse {} ({e}); using defaults",
                        path.display()
                    );
                    Settings::default()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                error!(
                    "failed to read {} ({e}); using defaults",
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
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scan_root, Some(PathBuf::from("/tmp/example")));
    }

    #[test]
    fn test_priority_encoder_round_trip() {
        let s = Settings {
            priority_image_encoder: Some("dinov2_base".into()),
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.priority_image_encoder, Some("dinov2_base".into()));
    }

    #[test]
    fn test_priority_encoder_default_is_none() {
        // Without an explicit user pick, the field must default to
        // None so the pipeline falls back to its default ordering.
        let s = Settings::default();
        assert!(s.priority_image_encoder.is_none());
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

    #[test]
    fn test_resolved_enabled_encoders_falls_back_to_default() {
        let s = Settings::default();
        let r = s.resolved_enabled_encoders();
        assert_eq!(r, DEFAULT_ENABLED_ENCODERS);
    }

    #[test]
    fn test_resolved_enabled_encoders_honours_user_pick() {
        let s = Settings {
            enabled_encoders: Some(vec!["clip_vit_b_32".into()]),
            ..Settings::default()
        };
        let r = s.resolved_enabled_encoders();
        assert_eq!(r, vec!["clip_vit_b_32".to_string()]);
    }

    #[test]
    fn test_resolved_enabled_encoders_strips_empty_strings() {
        let s = Settings {
            enabled_encoders: Some(vec!["".into(), "siglip2_base".into()]),
            ..Settings::default()
        };
        let r = s.resolved_enabled_encoders();
        assert_eq!(r, vec!["siglip2_base".to_string()]);
    }

    #[test]
    fn test_resolved_enabled_encoders_empty_list_falls_back() {
        // An explicit empty list is treated as "use the default" —
        // never as "disable everything," because that would silently
        // brick the app for any user who managed to clear all toggles.
        let s = Settings {
            enabled_encoders: Some(vec![]),
            ..Settings::default()
        };
        assert_eq!(s.resolved_enabled_encoders(), DEFAULT_ENABLED_ENCODERS);
    }
}
