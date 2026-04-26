//! Text encoder for OpenAI CLIP-based semantic search.
//!
//! Split into two submodules:
//! - [`encoder`] — the [`ClipTextEncoder`] struct, ONNX session
//!   setup, and `encode` / `encode_batch`. Tokenization uses the
//!   HuggingFace `tokenizers` crate (canonical Rust BPE/SentencePiece
//!   loader from `tokenizer.json`).
//! - [`pooling`] — output extraction and L2 normalisation helpers.
//!
//! The custom `SimpleTokenizer` (WordPiece with case-fallback lookup)
//! was removed when we switched from the multilingual CLIP text
//! encoder to OpenAI CLIP — multilingual used WordPiece, OpenAI uses
//! byte-level BPE, and the `tokenizers` crate handles both via
//! tokenizer.json without custom code. The crate also handles
//! SentencePiece for SigLIP-2.

pub mod encoder;
pub mod pooling;

pub use encoder::ClipTextEncoder;
