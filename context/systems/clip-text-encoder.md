# clip-text-encoder

*Maturity: working*

## Scope / Purpose

Encodes a text query into the same 512-dimensional embedding space as the image encoder, so that cosine similarity between a text embedding and image embeddings retrieves semantically matching images. Uses the multilingual CLIP variant (`clip-ViT-B-32-multilingual-v1`) — typing in any of 50+ languages produces an embedding that aligns with image content. Includes a pure-Rust WordPiece tokenizer that loads from a HuggingFace `tokenizer.json`, avoiding any C tokenizer dependency.

## Boundaries / Ownership

- **Owns:** the WordPiece tokenizer logic, vocabulary loading, special-token handling, sequence padding/truncation, multi-name output extraction, mean-pooling fallback.
- **Does not own:** the in-memory similarity index (delegates to `cosine-similarity`), the lazy-init lifecycle (lives in `tauri-commands` via `TextEncoderState`).
- **Public API:** `SimpleTokenizer::from_file(path)`, `TextEncoder::new(model_path, tokenizer_path)`, `encode(text)`, `inspect_model`.

## Current Implemented Reality

### Tokenizer (pure-Rust WordPiece)

Loads `tokenizer.json` and reads:
- `model.vocab` → `HashMap<String, i64>`
- `added_tokens[*]` → also added to vocab
- Special tokens: `[CLS]=101`, `[SEP]=102`, `[PAD]=0`, `[UNK]=100` (defaults; overridden if present in vocab).

Tokenisation is whitespace-split → per-word WordPiece longest-match-from-the-front. The first piece of a word stands alone; subsequent pieces get the `##` prefix. Source: `encoder_text.rs:113-170`.

### Case handling — the durable rationale

The multilingual tokenizer.json declares `lowercase: false`, but the vocabulary is mixed: some entries are stored lowercase, others in original case. The `tokenize_word` function tries the original case first, then a lowercase fallback before falling back to `[UNK]`. (`encoder_text.rs:113-156`.)

This was added in commit `930f1fc` (2025-12-17) — message: "Enhanced SimpleTokenizer to try both original and lowercase forms for WordPiece vocab lookup, improving multilingual support." Without this fallback, queries that happened to be uppercase or capitalised would tokenize to mostly `[UNK]` and produce useless embeddings.

### Model session

```rust
Session::builder()
    .with_execution_providers([CUDAExecutionProvider::default().build()])  // try CUDA
    .commit_from_file(model_path)                                          // CPU fallback in `Err` arm
```

Same fallback shape as `clip-image-encoder`. The text model is loaded lazily — see `tauri-commands` and `lib.rs:114-142`.

### Sequence handling

```text
encode(text):
    (input_ids, attention_mask) = tokenizer.encode(text, add_special_tokens=true)
    pad_or_truncate(input_ids, pad=tokenizer.pad_token_id())
    pad_or_truncate(attention_mask, pad=0)
    # both are now exactly length 128 (max_seq_length)

    Tensor::from_array(([1, 128], input_ids))
    Tensor::from_array(([1, 128], attention_mask))

    Session::run(input_ids, attention_mask)

    output_names = ["sentence_embedding", "text_embeds", "pooler_output", "last_hidden_state"]
    for name in output_names:
        if name in outputs:
            data = outputs[name].try_extract_tensor::<f32>().to_vec()
            if data.len() == 512:
                return data
            elif data.len() == max_seq_length * 768:
                # mean pool over sequence (DistilBERT-style backbone)
                return mean_pool(data, hidden_size=768)
            elif data.len() >= 512:
                # Take first 512 dims as a degraded fallback
                ...
    return Err  # if nothing matched
```

The four output names are tried in order because `clip-ViT-B-32-multilingual-v1` exports vary across versions and converters — sometimes the output is named `sentence_embedding`, sometimes `text_embeds`, etc. The mean-pool branch handles the case where the model returns a `[seq_len, hidden_size]` shape that needs to be pooled into a single 768-vector. (Read `encoder_text.rs:282-...` for the full attempt order.)

### `max_seq_length`

Hardcoded to 128 (`encoder_text.rs:223-224`) per the multilingual CLIP card. Anything longer is truncated.

## Key Interfaces / Data Flow

```text
tauri::semantic_search
    ──► (lazy init on first call)
        TextEncoder::new(models/model_text.onnx, models/tokenizer.json)
    ──► encoder.encode(query)
        ──► SimpleTokenizer::encode (WordPiece + case fallback)
        ──► pad_or_truncate to length 128
        ──► Session::run
        ──► extract by name + shape, mean-pool if needed
        ──► Vec<f32> length 512
    ──► Array1::from_vec(text_embedding)
    ──► CosineIndex::get_similar_images_sorted(...)
```

The encoder is held inside `Mutex<Option<TextEncoder>>` — see `tauri-commands.md` for the lazy-init protocol.

## Implemented Outputs / Artifacts

- 512-d `Vec<f32>` per query.
- Console log of `Loaded vocabulary with N tokens` and the special-token ids on first load (`encoder_text.rs:60-64`).

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Tokenizer is whitespace-only | A query with no spaces (e.g., `LLMresearch` or CJK languages without explicit whitespace separators) | The whole sequence becomes a single "word" and WordPiece longest-match operates on the full string. This works reasonably for CJK because the vocab includes single-character entries, but it is not how the reference Python tokenizer handles tokenisation pre-CJK. |
| Hardcoded special-token ids when vocab lookup fails | A non-standard tokenizer.json that omits `[CLS]`, `[SEP]`, `[PAD]`, or `[UNK]` | Falls back to ids `101`, `102`, `0`, `100` which match BERT-family conventions but may be wrong for other models. |
| First-call latency | First semantic search of a session | Several seconds — model load + tokenizer parse + (in dev) CPU fallback. The user sees no progress indicator beyond "Searching for ..." spinner. |
| Generic error message at the IPC boundary | Any failure in this module | Frontend shows "Search failed. Make sure the text model is available." regardless of whether it is a tokenizer load error, a CUDA initialisation hiccup, a missing output name, or a session.run failure. The actual `Err` is logged to stdout but not surfaced. |
| Mutex poisoning is unrecoverable | Any panic while holding `TextEncoderState.encoder.lock()` | Subsequent semantic searches all fail with `Mutex poisoned`. App restart is the only recovery. |

## Partial / In Progress

None.

## Planned / Missing / Likely Changes

- Better tokenisation for languages without whitespace word boundaries — at minimum, document the limitation; ideally, plug in a per-language pre-tokeniser.
- Surface the actual `Err` string back to the frontend (replace the generic "Search failed" message).
- Persist the loaded tokenizer (it is small) across runs; or pre-build the vocab lookup at compile time if the tokenizer.json is bundled.
- Optional: support a query-language hint to override the case-handling strategy when the user explicitly knows what they typed.

## Durable Notes / Discarded Approaches

- **Pure-Rust WordPiece was chosen over the `tokenizers` crate** — the `Cargo.toml` has a comment block explaining: "Tokenizer is now implemented in pure Rust within encoder_text.rs / No external tokenizer dependency needed!" The motivation is dependency hygiene: the `tokenizers` crate has C build dependencies that complicated cross-compilation. The pure-Rust implementation handles WordPiece correctly for the multilingual vocab in use; it does not handle BPE or SentencePiece, which is fine because the model uses WordPiece.
- **Multi-name output extraction is intentional** (`encoder_text.rs:282-...`). Different ONNX exports of the same model name the output differently. Trying four names in order avoids a re-export every time the upstream model gets re-converted by a different tool.
- **Mean-pool fallback for `[seq_len, hidden_size]`** is the DistilBERT-style backbone behaviour. The OpenAI CLIP text encoder usually returns a pooled output already, but multilingual exports sometimes return the full hidden states and require explicit pooling. The fallback path keeps the encoder model-agnostic.

## Obsolete / No Longer Relevant

None.
