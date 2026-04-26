use std::{collections::HashMap, error::Error, fs, path::Path};
use tracing::{debug, info};

/// Simple tokenizer that loads from HuggingFace tokenizer.json format.
/// This is a pure Rust implementation to avoid C library dependencies.
pub struct SimpleTokenizer {
    pub(super) vocab: HashMap<String, i64>,
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
