//! Cosine-similarity index split into focused submodules:
//!
//! - `math`  — pure helpers: the `cosine_similarity` formula and the
//!   `score_cmp_desc` comparator shared by all retrieval methods.
//! - `index` — the `CosineIndex` struct, embedding ingestion
//!   (`add_image`, `populate_from_db`), and the three retrieval
//!   methods (`get_similar_images`, `get_similar_images_sorted`,
//!   `get_tiered_similar_images`).
//! - `cache` — disk persistence: `save_to_disk` / `save_to_path` and
//!   `load_from_disk_if_fresh` / `load_from_path_if_fresh`.
//!
//! The struct lives in `index` and the cache impl block lives in
//! `cache`; both contribute to the same `CosineIndex` inherent impl,
//! so the public API stays exactly as it was when everything lived in
//! a single file. `cache` is brought into scope here only for its
//! `impl CosineIndex` side-effect.
//!
//! `cosine_similarity.rs` is preserved as a re-export shim so existing
//! `crate::similarity_and_semantic_search::cosine_similarity::CosineIndex`
//! imports continue to work without any caller changes.

mod cache;
pub mod index;
pub(crate) mod math;

pub use index::CosineIndex;
