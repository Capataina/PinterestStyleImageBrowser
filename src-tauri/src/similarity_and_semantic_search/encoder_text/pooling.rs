//! Output extraction and pooling helpers for the text encoder.
//!
//! The multilingual CLIP text model can be exported in several shapes
//! depending on how the ONNX export was produced. The encoder tries a
//! ranked list of output names and the helpers here classify the
//! resulting tensor shape into a single 512-dim embedding.

/// Classify a flat output tensor's shape and produce the 512-dim embedding
/// for a single example.
///
/// Returns `None` when the data length matches none of the known shapes —
/// the caller should keep trying other output names.
///
/// Supported shapes:
/// - `[1, 512]` (sentence_embedding / text_embeds / pooler_output) — used as is
/// - `[1, max_seq_length, 768]` (last_hidden_state for DistilBERT) — mean-pooled
/// - any length `>= 512` — first 512 dims taken as a fallback
pub fn try_extract_single_embedding(data: Vec<f32>, max_seq_length: usize) -> Option<Vec<f32>> {
    // Handle different output shapes:
    // - [1, 512] -> take as is
    // - [1, seq_len, hidden_size] -> take first token (CLS) or mean pool
    if data.len() == 512 {
        Some(data)
    } else if data.len() == max_seq_length * 768 {
        // Mean pooling over sequence for DistilBERT (768 hidden size)
        Some(mean_pool(&data, 768))
    } else if data.len() >= 512 {
        // Take first 512 dimensions
        Some(data[..512].to_vec())
    } else {
        None
    }
}

/// Normalize a vector to unit length (L2 normalization)
pub fn normalize(vec: &[f32]) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        vec.iter().map(|x| x / norm).collect()
    } else {
        vec.to_vec()
    }
}

/// Mean pooling over the sequence dimension.
///
/// `data` is laid out as `[seq_len, hidden_size]` in row-major order;
/// the result is a single `hidden_size`-length vector. If the model's
/// hidden size exceeds 512 the result is truncated to 512 dims —
/// for the multilingual CLIP text model the projection layer should
/// already produce 512-dim outputs, so this is a defensive trim.
pub fn mean_pool(data: &[f32], hidden_size: usize) -> Vec<f32> {
    let seq_len = data.len() / hidden_size;
    let mut pooled = vec![0.0f32; hidden_size];

    for i in 0..seq_len {
        for j in 0..hidden_size {
            pooled[j] += data[i * hidden_size + j];
        }
    }

    for val in pooled.iter_mut() {
        *val /= seq_len as f32;
    }

    // If we need 512 dims but have 768, we'd need a projection
    // For this model, the output should already be projected to 512
    if pooled.len() > 512 {
        pooled.truncate(512);
    }

    pooled
}
