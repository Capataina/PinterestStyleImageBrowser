//! Text encoder for CLIP-based semantic search.
//!
//! Split into three submodules:
//! - [`tokenizer`] — pure-Rust WordPiece tokenizer with case-fallback lookup
//! - [`encoder`] — the [`ClipTextEncoder`] struct, ONNX session setup, and `encode` / `encode_batch`
//! - [`pooling`] — output extraction, mean-pool, and L2 normalisation helpers
//!
//! The struct was renamed from `TextEncoder` to `ClipTextEncoder` when
//! the `super::encoders::TextEncoder` *trait* was introduced — the trait
//! and the original struct collided on the same name. Callers now use
//! `ClipTextEncoder` for the concrete type and `super::encoders::TextEncoder`
//! for the trait.

pub mod encoder;
pub mod pooling;
pub mod tokenizer;

pub use encoder::ClipTextEncoder;
pub use tokenizer::SimpleTokenizer;
