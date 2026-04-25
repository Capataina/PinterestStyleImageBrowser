use ort::{execution_providers::CUDAExecutionProvider, session::Session, value::Tensor};
use std::{collections::HashMap, error::Error, fs, path::Path};
use tracing::{debug, info, warn};

/// Simple tokenizer that loads from HuggingFace tokenizer.json format.
/// This is a pure Rust implementation to avoid C library dependencies.
pub struct SimpleTokenizer {
    vocab: HashMap<String, i64>,
    /// Reverse lookup table built at load time. Currently unused — the
    /// encoder only needs forward lookup — but kept because the cost of
    /// building it is negligible (~1 MB for the multilingual vocab) and
    /// future debugging features (decode token-ids back to text, dump
    /// the BPE pieces a query produced) would need it. `#[allow(dead_code)]`
    /// rather than removal so we don't have to re-add it later.
    #[allow(dead_code)]
    vocab_reverse: HashMap<i64, String>,
    cls_token_id: i64,
    sep_token_id: i64,
    pad_token_id: i64,
    unk_token_id: i64,
}

impl SimpleTokenizer {
    /// Load tokenizer from a tokenizer.json file
    pub fn from_file(path: &Path) -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;

        // Extract vocabulary from the model section
        let mut vocab = HashMap::new();
        let mut vocab_reverse = HashMap::new();

        // Try to get vocab from model.vocab (WordPiece/BPE format)
        if let Some(model_vocab) = json.get("model").and_then(|m| m.get("vocab")) {
            if let Some(vocab_obj) = model_vocab.as_object() {
                for (token, id) in vocab_obj {
                    if let Some(id_num) = id.as_i64() {
                        vocab.insert(token.clone(), id_num);
                        vocab_reverse.insert(id_num, token.clone());
                    }
                }
            }
        }

        // Also add any added_tokens
        if let Some(added_tokens) = json.get("added_tokens").and_then(|t| t.as_array()) {
            for token_info in added_tokens {
                if let (Some(content), Some(id)) = (
                    token_info.get("content").and_then(|c| c.as_str()),
                    token_info.get("id").and_then(|i| i.as_i64()),
                ) {
                    vocab.insert(content.to_string(), id);
                    vocab_reverse.insert(id, content.to_string());
                }
            }
        }

        if vocab.is_empty() {
            return Err("Failed to load vocabulary from tokenizer.json".into());
        }

        // Find special token IDs
        let cls_token_id = *vocab.get("[CLS]").unwrap_or(&101);
        let sep_token_id = *vocab.get("[SEP]").unwrap_or(&102);
        let pad_token_id = *vocab.get("[PAD]").unwrap_or(&0);
        let unk_token_id = *vocab.get("[UNK]").unwrap_or(&100);

        info!("Loaded vocabulary with {} tokens", vocab.len());
        debug!(
            "Special tokens - CLS: {}, SEP: {}, PAD: {}, UNK: {}",
            cls_token_id, sep_token_id, pad_token_id, unk_token_id
        );

        Ok(SimpleTokenizer {
            vocab,
            vocab_reverse,
            cls_token_id,
            sep_token_id,
            pad_token_id,
            unk_token_id,
        })
    }

    /// Tokenize text into token IDs
    /// Note: The multilingual CLIP tokenizer uses lowercase: false per tokenizer.json,
    /// but for WordPiece lookup we need to try both original case and lowercase
    /// since the vocab may contain either form.
    pub fn encode(&self, text: &str, add_special_tokens: bool) -> (Vec<i64>, Vec<i64>) {
        let mut input_ids = Vec::new();
        let mut attention_mask = Vec::new();

        // Add [CLS] token
        if add_special_tokens {
            input_ids.push(self.cls_token_id);
            attention_mask.push(1);
        }

        // Simple whitespace + subword tokenization
        // Keep original case as the model uses lowercase: false
        let words: Vec<&str> = text.split_whitespace().collect();

        for word in words {
            let word_tokens = self.tokenize_word(word);
            for token_id in word_tokens {
                input_ids.push(token_id);
                attention_mask.push(1);
            }
        }

        // Add [SEP] token
        if add_special_tokens {
            input_ids.push(self.sep_token_id);
            attention_mask.push(1);
        }

        (input_ids, attention_mask)
    }

    /// Tokenize a single word using WordPiece-style tokenization
    /// Tries original case first, then lowercase as fallback for vocab lookup
    fn tokenize_word(&self, word: &str) -> Vec<i64> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = word.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let mut end = chars.len();
            let mut found = false;

            while start < end {
                // Build substring for this position
                let substr_base: String = chars[start..end].iter().collect();
                let substr: String = if start == 0 {
                    substr_base.clone()
                } else {
                    format!("##{}", substr_base)
                };

                // Try original case first
                if let Some(&token_id) = self.vocab.get(&substr) {
                    tokens.push(token_id);
                    found = true;
                    start = end;
                    break;
                }

                // Try lowercase as fallback (some multilingual vocabs have mixed case)
                let substr_lower: String = if start == 0 {
                    substr_base.to_lowercase()
                } else {
                    format!("##{}", substr_base.to_lowercase())
                };

                if substr_lower != substr {
                    if let Some(&token_id) = self.vocab.get(&substr_lower) {
                        tokens.push(token_id);
                        found = true;
                        start = end;
                        break;
                    }
                }

                end -= 1;
            }

            if !found {
                // Character not in vocabulary, use [UNK]
                tokens.push(self.unk_token_id);
                start += 1;
            }
        }

        if tokens.is_empty() {
            tokens.push(self.unk_token_id);
        }

        tokens
    }

    pub fn pad_token_id(&self) -> i64 {
        self.pad_token_id
    }
}

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
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initializing Text Encoder ===");
        info!("Model path: {:?}", model_path);
        info!("Tokenizer path: {:?}", tokenizer_path);

        // Load the tokenizer
        info!("Loading tokenizer...");
        let tokenizer = SimpleTokenizer::from_file(tokenizer_path)?;
        info!("✓ Tokenizer loaded successfully");

        // Try to build with CUDA, fall back to CPU
        info!("Attempting to enable CUDA...");
        let builder_result = Session::builder()?
            .with_execution_providers([CUDAExecutionProvider::default().build()]);

        let session = match builder_result {
            Ok(builder) => {
                info!("✓ CUDA execution provider accepted");
                let session = builder.commit_from_file(model_path)?;
                info!("✓ Text encoder session created with CUDA");
                session
            }
            Err(e) => {
                warn!("✗ CUDA execution provider failed: {}", e);
                info!("Falling back to CPU...");
                let session = Session::builder()?.commit_from_file(model_path)?;
                info!("✓ Text encoder session created with CPU");
                session
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

                // Handle different output shapes:
                // - [1, 512] -> take as is
                // - [1, seq_len, hidden_size] -> take first token (CLS) or mean pool
                if data.len() == 512 {
                    embedding = Some(data);
                    break;
                } else if data.len() == self.max_seq_length * 768 {
                    // Mean pooling over sequence for DistilBERT (768 hidden size)
                    embedding = Some(Self::mean_pool(&data, 768));
                    break;
                } else if data.len() >= 512 {
                    // Take first 512 dimensions
                    embedding = Some(data[..512].to_vec());
                    break;
                }
            }
        }

        let embedding = embedding.ok_or("Could not extract embedding from model output")?;

        // Normalize the embedding (CLIP embeddings should be unit vectors)
        let normalized = Self::normalize(&embedding);

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
                        let emb = Self::normalize(&data[start..end].to_vec());
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

    /// Normalize a vector to unit length (L2 normalization)
    fn normalize(vec: &[f32]) -> Vec<f32> {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            vec.iter().map(|x| x / norm).collect()
        } else {
            vec.to_vec()
        }
    }

    /// Mean pooling over sequence dimension
    fn mean_pool(data: &[f32], hidden_size: usize) -> Vec<f32> {
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
