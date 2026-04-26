//! Typed error returned by every Tauri command.
//!
//! Replaces the previous `Result<T, String>` pattern where every
//! command did `.map_err(|e| e.to_string())` and the frontend got back
//! an opaque string like "Failed to fetch image: error returned from
//! database: ...". The user couldn't tell whether the cause was
//! "tokenizer parse failed", "model file missing", "DB Mutex poisoned",
//! "ONNX session error", or "image path not found".
//!
//! With `ApiError` the frontend gets a discriminated union:
//!
//! ```json
//! { "kind": "tokenizer_missing", "details": "/Users/.../tokenizer.json" }
//! { "kind": "db", "details": "no such row" }
//! { "kind": "encoder", "details": "ONNX session creation failed: ..." }
//! ```
//!
//! …and can branch on `kind` to pick a specific UX response — a
//! missing model could trigger a re-download dialog, a DB lock could
//! be retried, etc.
//!
//! The `#[serde(tag = "kind", content = "details", rename_all = "snake_case")]`
//! attribute is what makes the JSON shape stable. Adding a new variant
//! is forward-compatible: the frontend handles unknown kinds via a
//! `default` case in its switch, and existing kinds keep their wire
//! shape.

use serde::Serialize;

/// Audit Inconsistent-Patterns finding: typed errors instead of
/// `Result<_, String>`. This is the only audit finding that was
/// flagged "decision-required" because the frontend's error display
/// switches from raw string to discriminated union — both halves
/// (backend + frontend) ship together in this commit.
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", content = "details", rename_all = "snake_case")]
pub enum ApiError {
    /// One of the model files isn't on disk. The frontend uses this to
    /// trigger a re-download dialog. The `details` payload is the
    /// expected absolute path.
    TokenizerMissing(String),
    TextModelMissing(String),
    ImageModelMissing(String),

    /// Anything from the SQLite layer — connection failures, query
    /// errors, prepared-statement issues. The `details` payload is
    /// the rusqlite error message.
    Db(String),

    /// CLIP image or text encoder failures — ONNX session errors,
    /// preprocessing failures, tensor shape mismatches.
    Encoder(String),

    /// Cosine index issues — Mutex poison, empty index, lookup
    /// failures.
    Cosine(String),

    /// Generic "thing the user asked about doesn't exist" — usually
    /// "image with id N not in DB" or "tag with id N not in DB".
    /// The `details` payload describes which resource.
    NotFound(String),

    /// Validation failure on user-supplied input (empty query string,
    /// negative top_n, etc.).
    BadInput(String),

    /// Filesystem operations — file read errors, missing thumbnails,
    /// I/O failures during the indexing pipeline.
    Io(String),

    /// Catch-all for anything that doesn't fit the named categories.
    /// Avoids a fallback to stringly-typed errors at the boundary —
    /// the frontend can still show a generic toast for these without
    /// losing structure.
    Internal(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::TokenizerMissing(p) => write!(f, "tokenizer file missing at {p}"),
            ApiError::TextModelMissing(p) => write!(f, "text model missing at {p}"),
            ApiError::ImageModelMissing(p) => write!(f, "image model missing at {p}"),
            ApiError::Db(m) => write!(f, "database error: {m}"),
            ApiError::Encoder(m) => write!(f, "encoder error: {m}"),
            ApiError::Cosine(m) => write!(f, "cosine error: {m}"),
            ApiError::NotFound(r) => write!(f, "not found: {r}"),
            ApiError::BadInput(m) => write!(f, "bad input: {m}"),
            ApiError::Io(m) => write!(f, "io error: {m}"),
            ApiError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for ApiError {}

// =====================================================================
// From-impls for the common error sources, so command bodies can use
// `?` directly without per-call .map_err() boilerplate.
// =====================================================================

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        // QueryReturnedNoRows is structurally a "not found" — surface it
        // as such so the frontend can branch on the typed kind instead
        // of string-matching on the message.
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            ApiError::NotFound("database row".to_string())
        } else {
            ApiError::Db(e.to_string())
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Io(e.to_string())
    }
}

/// Convenience for the many `cosine_state.index.lock().map_err(...)`
/// sites — Mutex poison errors all become Cosine errors.
impl<T> From<std::sync::PoisonError<T>> for ApiError {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        ApiError::Cosine(format!("mutex poisoned: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_with_kind_and_details() {
        let e = ApiError::TokenizerMissing("/path/to/tok.json".into());
        let json = serde_json::to_string(&e).unwrap();
        // The frontend depends on this exact wire shape.
        assert!(json.contains("\"kind\":\"tokenizer_missing\""));
        assert!(json.contains("\"details\":\"/path/to/tok.json\""));
    }

    #[test]
    fn serialises_db_kind() {
        let e = ApiError::Db("constraint violation".into());
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"kind\":\"db\""));
    }

    #[test]
    fn rusqlite_no_rows_becomes_not_found() {
        let e: ApiError = rusqlite::Error::QueryReturnedNoRows.into();
        assert!(matches!(e, ApiError::NotFound(_)));
    }

    #[test]
    fn rusqlite_other_becomes_db() {
        // Any non-NoRows variant maps to ApiError::Db.
        let e: ApiError =
            rusqlite::Error::InvalidColumnIndex(99).into();
        assert!(matches!(e, ApiError::Db(_)));
    }

    #[test]
    fn display_includes_kind_label() {
        let e = ApiError::Encoder("ONNX failed".into());
        let s = format!("{e}");
        assert!(s.contains("encoder error"));
        assert!(s.contains("ONNX failed"));
    }
}
