use ort::{session::Session, value::Tensor};
use std::{error::Error, path::Path};
use tracing::{debug, info, warn};

use super::pooling::{normalize, try_extract_single_embedding};
use super::tokenizer::SimpleTokenizer;

// CoreML is intentionally NOT used for the text encoder on macOS.
// The multilingual CLIP text model is a transformer (DistilBERT-based,
// 383 nodes); CoreML can only execute ~17 of those nodes natively and
// falls back to CPU for the rest. We measured 6-15s of upfront CoreML
// graph-analysis cost per session-create, plus partition management
// overhead at run time — all for a worse runtime than plain CPU.
//
// Image encoder (encoder.rs) keeps CoreML because CLIP's CNN-heavy
// graph is what CoreML is good at — there it's a 5-10x win.

#[cfg(not(target_os = "macos"))]
use ort::execution_providers::CUDAExecutionProvider;

/// Text encoder for CLIP-based semantic search.
/// Uses the multilingual CLIP model (clip-ViT-B-32-multilingual-v1) to encode text
/// into the same 512-dimensional embedding space as images.
pub struct TextEncoder {
    session: Session,
    tokenizer: SimpleTokenizer,
    max_seq_length: usize,
}

impl TextEncoder {
    /// Create a new TextEncoder from ONNX model and tokenizer files.
    ///
    /// # Arguments
    /// * `model_path` - Path to the ONNX text model (e.g., "models/model_text.onnx")
    /// * `tokenizer_path` - Path to the tokenizer.json file (e.g., "models/tokenizer.json")
    #[cfg(target_os = "macos")]
    fn build_session_with_accel(model_path: &Path) -> Result<Session, Box<dyn Error>> {
        // macOS: text encoder runs CPU-only because CoreML coverage is
        // poor for transformers (see top-of-file comment). Plain CPU
        // session creates in ~1-2s vs CoreML's 6-15s for the same model.
        info!("text encoder using CPU (CoreML skipped — poor transformer coverage)");
        let session = Session::builder()?.commit_from_file(model_path)?;
        Ok(session)
    }

    #[cfg(not(target_os = "macos"))]
    fn build_session_with_accel(model_path: &Path) -> Result<Session, Box<dyn Error>> {
        info!("trying CUDA execution provider for text encoder");
        let session = Session::builder()?
            .with_execution_providers([CUDAExecutionProvider::default().build()])?
            .commit_from_file(model_path)?;
        Ok(session)
    }

    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initializing text encoder ===");
        debug!("Model path: {:?}", model_path);
        debug!("Tokenizer path: {:?}", tokenizer_path);

        // Load the tokenizer
        info!("Loading tokenizer...");
        let tokenizer = SimpleTokenizer::from_file(tokenizer_path)?;
        info!("Tokenizer loaded ({} tokens)", tokenizer.vocab.len());

        // Build the ONNX session with the platform-appropriate
        // execution provider, falling back to CPU on any error. Same
        // story as the image encoder: ort lets the provider registration
        // succeed even when the EP isn't actually available, so we
        // can't claim "running on CoreML" or "running on CUDA" with
        // confidence. We just log what we tried.
        let session = match Self::build_session_with_accel(model_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("text encoder accelerator init failed ({e}); falling back to CPU");
                Session::builder()?.commit_from_file(model_path)?
            }
        };

        // The multilingual CLIP model uses max_seq_length of 128
        let max_seq_length = 128;

        Ok(TextEncoder {
            session,
            tokenizer,
            max_seq_length,
        })
    }

    /// Inspect the model's input and output names (useful for debugging)
    pub fn inspect_model(&self) {
        debug!("Text Model inputs:");
        for input in self.session.inputs.iter() {
            debug!("  Name: {:?}", input.name);
        }

        debug!("\nText Model outputs:");
        for output in self.session.outputs.iter() {
            debug!("  Name: {:?}", output.name);
        }
    }

    /// Encode a text string into a 512-dimensional embedding vector.
    ///
    /// # Arguments
    /// * `text` - The text to encode (supports 50+ languages)
    ///
    /// # Returns
    /// A 512-dimensional normalized embedding vector
    #[tracing::instrument(name = "clip.encode_text", skip(self, text), fields(query_len = text.len()))]
    pub fn encode(&mut self, text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
        // Tokenize the text
        let (mut input_ids, mut attention_mask) = self.tokenizer.encode(text, true);

        // Pad or truncate to max_seq_length
        self.pad_or_truncate(&mut input_ids, self.tokenizer.pad_token_id());
        self.pad_or_truncate(&mut attention_mask, 0);

        // Create tensors with shape [1, max_seq_length]
        let input_ids_tensor: Tensor<i64> =
            Tensor::from_array(([1usize, self.max_seq_length], input_ids))?;
        let attention_mask_tensor: Tensor<i64> =
            Tensor::from_array(([1usize, self.max_seq_length], attention_mask))?;

        // Run inference
        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor
        ])?;

        // Try different possible output names for the text embedding
        // Common names: "text_embeds", "sentence_embedding", "last_hidden_state", "pooler_output"
        let output_names = [
            "sentence_embedding",
            "text_embeds",
            "pooler_output",
            "last_hidden_state",
        ];

        let mut embedding: Option<Vec<f32>> = None;

        for name in output_names {
            if let Some(tensor) = outputs.get(name) {
                let (_shape, data_view) = tensor.try_extract_tensor::<f32>()?;
                let data = data_view.to_vec();

                if let Some(emb) = try_extract_single_embedding(data, self.max_seq_length) {
                    embedding = Some(emb);
                    break;
                }
            }
        }

        let embedding = embedding.ok_or("Could not extract embedding from model output")?;

        // Normalize the embedding (CLIP embeddings should be unit vectors)
        let normalized = normalize(&embedding);

        Ok(normalized)
    }

    /// Encode multiple texts in a batch (more efficient for multiple queries)
    pub fn encode_batch(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let batch_size = texts.len();

        // Tokenize all texts
        let mut all_input_ids: Vec<i64> = Vec::with_capacity(batch_size * self.max_seq_length);
        let mut all_attention_masks: Vec<i64> =
            Vec::with_capacity(batch_size * self.max_seq_length);

        for text in texts {
            let (mut input_ids, mut attention_mask) = self.tokenizer.encode(text, true);

            self.pad_or_truncate(&mut input_ids, self.tokenizer.pad_token_id());
            self.pad_or_truncate(&mut attention_mask, 0);

            all_input_ids.extend(input_ids);
            all_attention_masks.extend(attention_mask);
        }

        // Create batch tensors
        let input_ids_tensor: Tensor<i64> =
            Tensor::from_array(([batch_size, self.max_seq_length], all_input_ids))?;
        let attention_mask_tensor: Tensor<i64> =
            Tensor::from_array(([batch_size, self.max_seq_length], all_attention_masks))?;

        // Run inference
        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor
        ])?;

        // Extract embeddings
        let output_names = ["sentence_embedding", "text_embeds", "pooler_output"];

        for name in output_names {
            if let Some(tensor) = outputs.get(name) {
                let (shape, data_view) = tensor.try_extract_tensor::<f32>()?;
                let data = data_view.to_vec();

                // Expected shape: [batch_size, 512]
                if shape.len() == 2 && shape[1] == 512 {
                    let mut embeddings = Vec::with_capacity(batch_size);
                    for i in 0..batch_size {
                        let start = i * 512;
                        let end = start + 512;
                        let emb = normalize(&data[start..end].to_vec());
                        embeddings.push(emb);
                    }
                    return Ok(embeddings);
                }
            }
        }

        Err("Could not extract batch embeddings from model output".into())
    }

    /// Pad or truncate a vector to max_seq_length
    fn pad_or_truncate(&self, vec: &mut Vec<i64>, pad_value: i64) {
        if vec.len() > self.max_seq_length {
            vec.truncate(self.max_seq_length);
        } else {
            vec.resize(self.max_seq_length, pad_value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_encoder() -> Option<TextEncoder> {
        let model_path = Path::new("models/model_text.onnx");
        let tokenizer_path = Path::new("models/tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            println!("Model or tokenizer not found, skipping test");
            return None;
        }

        TextEncoder::new(model_path, tokenizer_path).ok()
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        }
    }

    #[test]
    fn test_inspect_model() {
        if let Some(encoder) = get_test_encoder() {
            encoder.inspect_model();
        }
    }

    #[test]
    fn test_encode_simple_text() {
        let Some(mut encoder) = get_test_encoder() else {
            return;
        };

        let embedding = encoder.encode("a photo of a dog").unwrap();

        // Should be 512 dimensions
        assert_eq!(embedding.len(), 512, "Embedding should be 512-dimensional");

        // Should be normalized (unit vector)
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "Embedding should be normalized, got norm: {}",
            norm
        );

        // Values should be finite
        for val in &embedding {
            assert!(val.is_finite(), "All values should be finite");
        }

        println!("Embedding norm: {}", norm);
        println!("First 10 values: {:?}", &embedding[..10]);
    }

    #[test]
    fn test_encode_multilingual() {
        let Some(mut encoder) = get_test_encoder() else {
            return;
        };

        // Test different languages
        let texts = [
            ("English", "a dog"),
            ("German", "ein Hund"),
            ("Spanish", "un perro"),
            ("French", "un chien"),
            ("Japanese", "犬"),
            ("Chinese", "狗"),
        ];

        for (lang, text) in texts {
            let embedding = encoder.encode(text);
            assert!(
                embedding.is_ok(),
                "Failed to encode {} text: {}",
                lang,
                text
            );
            let emb = embedding.unwrap();
            assert_eq!(emb.len(), 512, "{} embedding should be 512-dim", lang);
            println!("✓ {} ('{}') encoded successfully", lang, text);
        }
    }

    #[test]
    fn test_similar_concepts_close_embeddings() {
        let Some(mut encoder) = get_test_encoder() else {
            return;
        };

        let dog = encoder.encode("a dog").unwrap();
        let puppy = encoder.encode("a puppy").unwrap();
        let cat = encoder.encode("a cat").unwrap();
        let car = encoder.encode("a car").unwrap();

        let sim_dog_puppy = cosine_similarity(&dog, &puppy);
        let sim_dog_cat = cosine_similarity(&dog, &cat);
        let sim_dog_car = cosine_similarity(&dog, &car);

        println!("dog <-> puppy: {:.4}", sim_dog_puppy);
        println!("dog <-> cat:   {:.4}", sim_dog_cat);
        println!("dog <-> car:   {:.4}", sim_dog_car);

        // Dog and puppy should be more similar than dog and car
        assert!(
            sim_dog_puppy > sim_dog_car,
            "Dog-puppy ({:.4}) should be more similar than dog-car ({:.4})",
            sim_dog_puppy,
            sim_dog_car
        );
    }

    #[test]
    fn test_encode_consistency() {
        let Some(mut encoder) = get_test_encoder() else {
            return;
        };

        let text = "a beautiful sunset over the ocean";
        let emb1 = encoder.encode(text).unwrap();
        let emb2 = encoder.encode(text).unwrap();

        // Same text should produce identical embeddings
        let similarity = cosine_similarity(&emb1, &emb2);
        assert!(
            (similarity - 1.0).abs() < 1e-5,
            "Same text should produce identical embeddings, got similarity: {}",
            similarity
        );
    }

    #[test]
    fn test_encode_batch() {
        let Some(mut encoder) = get_test_encoder() else {
            return;
        };

        let texts = ["a dog", "a cat", "a car"];
        let batch_embeddings = encoder.encode_batch(&texts).unwrap();

        assert_eq!(batch_embeddings.len(), 3);

        // Compare with individual encodings
        for (i, text) in texts.iter().enumerate() {
            let individual = encoder.encode(text).unwrap();
            let batch = &batch_embeddings[i];

            let similarity = cosine_similarity(&individual, batch);
            assert!(
                similarity > 0.99,
                "Batch and individual encoding should match for '{}', got similarity: {}",
                text,
                similarity
            );
        }
    }

    #[test]
    fn test_encoding_speed() {
        use std::time::Instant;

        let Some(mut encoder) = get_test_encoder() else {
            return;
        };

        let text = "a photo of a dog playing in the park";

        // Warm up
        let _ = encoder.encode(text);

        // Time 10 encodings
        let start = Instant::now();
        for _ in 0..10 {
            let _ = encoder.encode(text).unwrap();
        }
        let elapsed = start.elapsed();

        let avg_ms = elapsed.as_millis() as f64 / 10.0;
        println!("Average encoding time: {:.2}ms", avg_ms);

        // Should be reasonably fast (< 500ms per encoding on CPU)
        assert!(
            avg_ms < 500.0,
            "Encoding should be < 500ms, got {:.2}ms",
            avg_ms
        );
    }
}
