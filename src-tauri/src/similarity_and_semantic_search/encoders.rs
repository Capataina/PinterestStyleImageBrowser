//! Encoder trait abstractions.
//!
//! Lets the project hold multiple image encoders (OpenAI CLIP, SigLIP-2,
//! DINOv2, future...) and multiple text encoders behind a common
//! interface. The search layer dispatches by the user's selected
//! encoder without knowing the concrete type.
//!
//! Each concrete encoder produces a fixed-dimensional embedding (CLIP
//! ViT-B/32 is 512-d; SigLIP-2 Base 256 and DINOv2-Base are both 768-d).
//! Cosine similarity assumes L2-normalised vectors, so every
//! implementation must normalise before returning.
//!
//! ## Why two traits, not one
//!
//! Image encoders take a path-on-disk and produce an embedding. Text
//! encoders take a string and produce an embedding. Some models (the
//! CLIP family) ship both as a paired set sharing one embedding space;
//! some models (DINOv2) only have an image encoder. Splitting the
//! traits lets the runtime mix-and-match — e.g., "use SigLIP-2 for
//! text→image, DINOv2 for image→image" — which is the dual-encoder
//! pattern Pinterest/Spotify use in production.
//!
//! ## Object safety
//!
//! Both traits are object-safe (no generic methods, no Self return
//! values) so they can be held as `Box<dyn ImageEncoder>` /
//! `Box<dyn TextEncoder>` in Tauri-managed state. This is what enables
//! the "user picks encoder in Settings" UX.

use std::error::Error;
use std::path::Path;

/// Image encoder — turns an image file into a fixed-dimensional
/// embedding for cosine-similarity search.
///
/// Implementations must:
/// - Return L2-normalised vectors (callers assume `|emb| == 1`).
/// - Be `Send` so they can live behind a `Mutex` in Tauri state.
/// - Use `&mut self` because ONNX session state typically isn't
///   thread-safe for concurrent use; sharing across threads requires
///   a lock anyway.
pub trait ImageEncoder: Send {
    /// Encode a single image into a normalised embedding.
    fn encode(&mut self, image_path: &Path) -> Result<Vec<f32>, Box<dyn Error>>;

    /// Encode a batch of images. Default implementation calls `encode`
    /// in a loop; concrete encoders should override for batch
    /// efficiency (single ONNX session call vs N).
    fn encode_batch(
        &mut self,
        image_paths: &[&Path],
    ) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
        image_paths.iter().map(|p| self.encode(p)).collect()
    }

    /// Output embedding dimension. Used by the cosine layer to
    /// validate that query and corpus vectors match.
    fn embedding_dim(&self) -> usize;

    /// Stable identifier for this encoder. Used as the database
    /// embedding-column suffix (`embedding_clip`, `embedding_siglip2`,
    /// `embedding_dinov2`) and as the user-facing label in the
    /// Settings encoder picker. Must be a valid SQL identifier
    /// fragment — `[a-z0-9_]+` only.
    fn id(&self) -> &'static str;
}

/// Text encoder — turns a text string into a fixed-dimensional
/// embedding in the same space as its paired `ImageEncoder`.
///
/// Note: not every image encoder has a paired text encoder. DINOv2
/// is image-only; queries that need text input must use a CLIP-family
/// text encoder paired with a CLIP-family image encoder.
pub trait TextEncoder: Send {
    /// Encode a query string into a normalised embedding.
    fn encode(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn Error>>;

    /// Output embedding dimension — must match the paired
    /// `ImageEncoder::embedding_dim` so cosine similarity is defined
    /// between text queries and image embeddings.
    fn embedding_dim(&self) -> usize;

    /// Stable identifier (see `ImageEncoder::id`).
    fn id(&self) -> &'static str;
}
