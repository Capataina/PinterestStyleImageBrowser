//! Text encoder for CLIP-based semantic search.
//!
//! Split into three submodules:
//! - [`tokenizer`] — pure-Rust WordPiece tokenizer with case-fallback lookup
//! - [`encoder`] — the [`TextEncoder`] struct, ONNX session setup, and `encode` / `encode_batch`
//! - [`pooling`] — output extraction, mean-pool, and L2 normalisation helpers
//!
//! The public surface is preserved via re-exports below — callers continue
//! to use `similarity_and_semantic_search::encoder_text::TextEncoder` and
//! `similarity_and_semantic_search::encoder_text::SimpleTokenizer` exactly
//! as before.

pub mod encoder;
pub mod pooling;
pub mod tokenizer;

pub use encoder::TextEncoder;
pub use tokenizer::SimpleTokenizer;
