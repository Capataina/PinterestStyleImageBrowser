//! Re-export shim preserving the original module path.
//!
//! The implementation moved into `super::cosine` (split across
//! `math.rs`, `index.rs`, and `cache.rs`); this module re-exports the
//! public surface so existing
//! `crate::similarity_and_semantic_search::cosine_similarity::CosineIndex`
//! imports in `lib.rs`, `indexing.rs`, `watcher.rs`, and the
//! integration test crate keep compiling unchanged.
pub use crate::similarity_and_semantic_search::cosine::*;
