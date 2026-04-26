# siglip2-encoder

*Maturity: working*

## Scope / Purpose

Google's sigmoid-loss CLIP-family model (ICCV 2025). Image and text branches in a shared 768-dim embedding space. Better English text-to-image alignment than OpenAI CLIP at every scale; uses a 256k-vocab Gemma SentencePiece tokenizer with much better cross-lingual coverage than CLIP's 49k BPE.

Both halves ship in one Rust file because they're trained together and only meaningful when used as a pair (mixing SigLIP image + CLIP text would put the queries in different embedding spaces). The encoder picker treats SigLIP-2 as a single user-selectable choice that activates both branches.

The user-facing pitch in `commands/encoders.rs` is: "Recommended for the 'Semantic Search' text query feature. Image branch is also strong; pick this if you want one encoder for both directions."

## Boundaries / Ownership

- **Owns:** image preprocessing (exact-square 256×256 bilinear, NO crop, mean=std=0.5 → [-1, 1]), text preprocessing (Gemma SentencePiece via `tokenizers` crate, pad to exactly 64 with id 0, NO attention_mask in the ONNX call), both ONNX session lifecycles (CPU only — neither branch is CoreML-tested), `pooler_output` extraction (MAP head — NOT CLS), L2-normalize at output, the encoder URL + filename + ID constants.
- **Does not own:** writing embeddings to disk (delegates to `db.upsert_embedding(..., encoder_id="siglip2_base", ...)`), the cosine cache (delegates to `cosine-similarity`), the model files on disk (delegates to `paths::models_dir()` + `SIGLIP2_*_FILENAME`).
- **Public API:**
  - `Siglip2ImageEncoder::new(model_path)` plus `ImageEncoder` trait impl
  - `Siglip2TextEncoder::new(model_path, tokenizer_path)` plus `TextEncoder` trait impl, with `tokenizer_for_diagnostic()` and `max_seq_length()` accessors mirroring `ClipTextEncoder`
  - Constants: `SIGLIP2_IMAGE_MODEL_URL`, `SIGLIP2_IMAGE_MODEL_FILENAME`, `SIGLIP2_TEXT_MODEL_URL`, `SIGLIP2_TEXT_MODEL_FILENAME`, `SIGLIP2_TOKENIZER_URL`, `SIGLIP2_TOKENIZER_FILENAME`, `SIGLIP2_ENCODER_ID`

## Current Implemented Reality

### Image branch — exact-square stretching (no crop)

```text
ImageReader::open → with_guessed_format → decode → to_rgb8
    │
    ▼
Resize to EXACTLY 256×256 (Triangle filter — image-rs's bilinear).
    NO aspect preservation. SigLIP-2 was trained on stretched-square inputs;
    aspect-preserving + crop would deviate from training-time geometry.
    │
    ▼
NO center-crop step. The full 256×256 stretched image is what the model sees.
    This is the only preprocessing pipeline in the project that doesn't crop —
    significant for queries where edge content matters (color, scenery, framing).
    See notes/preprocessing-spatial-coverage.md.
    │
    ▼
Split RGB into 3 contiguous slices (CHW layout)
    │
    ▼
Normalise per-channel: x = (x/255 - 0.5) / 0.5
    Maps [0, 1] → [-1, 1]. Distinct from CLIP-stats and ImageNet-stats.
    │
    ▼
Reshape to ndarray::Array4 with shape (1, 3, 256, 256)
    │
    ▼
Session::run with ONE input:
    pixel_values  ← image tensor [1, 3, 256, 256]
    │
    ▼
Output: pooler_output, shape [1, 768]
    The MAP (Multi-head Attention Pooling) head's projected output.
    NOT last_hidden_state[:, 0, :] — SigLIP doesn't use CLS-token pooling.
    │
    ▼
L2-normalize → return Vec<f32> length 768
```

### Text branch — Gemma SentencePiece, fixed 64-length, NO attention_mask

```text
text query
    │
    ▼
Tokenizer::from_file(siglip2_tokenizer.json) loads the Gemma 256k-vocab tokenizer
    Specials: <pad>=0, <eos>=1 (auto-appended), <bos>=2 (NOT prepended), <unk>=3
    │
    ▼
encode(text, true) → Encoding
    auto-appends <eos>; does not prepend <bos>
    no padding by the tokenizer itself
    │
    ▼
encode_text() pads/truncates to EXACTLY 64 with PAD_TOKEN_ID=0
    The model's position embedding is fixed-size 64. Longer queries would crash
    with an out-of-range Gather at runtime — the encoder hard-caps before
    sending to ONNX.
    │
    ▼
Session::run with ONE input (NO attention_mask):
    input_ids  ← int64 tensor [1, 64]
    
    The text encoder's ONNX export inlined the position lookup as a fixed-length
    Slice, so attention_mask is unused. Passing it would error from the session.
    Verified in the prior research pass by inspecting the ONNX graph header.
    │
    ▼
Output: pooler_output, shape [1, 768]   (same head shape as the image branch)
    │
    ▼
L2-normalize → return Vec<f32> length 768
```

### Three preprocessing differences vs CLIP / DINOv2

| Aspect | CLIP | DINOv2 | SigLIP-2 |
|--------|------|--------|----------|
| Image size | 224×224 | 224×224 | **256×256** |
| Resize geometry | aspect-preserving + center-crop | aspect-preserving + center-crop | **exact-square stretch, NO crop** |
| Resize filter | bicubic (CatmullRom) | bicubic (CatmullRom) | **bilinear (Triangle)** |
| Mean | CLIP-native | ImageNet | **[0.5, 0.5, 0.5]** |
| Std | CLIP-native | ImageNet | **[0.5, 0.5, 0.5]** (maps to [-1, 1]) |
| Image input shape | [1, 3, 224, 224] | [1, 3, 224, 224] | [1, 3, 256, 256] |
| Image output | `image_embeds` 512-d | CLS slice from `last_hidden_state` 768-d | `pooler_output` (MAP head) 768-d |
| Text tokenizer | BPE 49k | n/a | **SentencePiece 256k (Gemma)** |
| Text max length | 77 | n/a | **64 (HARD CAP)** |
| Text pad token id | 49407 | n/a | **0** |
| Text inputs | input_ids + attention_mask | n/a | **input_ids ONLY** |
| Text output | `text_embeds` 512-d | n/a | `pooler_output` 768-d |

The user's open concern in `notes/preprocessing-spatial-coverage.md` references this table specifically: SigLIP-2 is the only encoder that sees the full image, which makes it the natural target for color/scenery queries.

## Key Interfaces / Data Flow

### Image branch (indexing path)

```
indexing.rs::run_trait_encoder("siglip2_base", make_encoder=Siglip2ImageEncoder::new):
    siglip_path = paths::models_dir().join(SIGLIP2_IMAGE_MODEL_FILENAME)  // "siglip2_vision.onnx"
    if siglip_path.exists():
        needs = db.get_images_without_embedding_for("siglip2_base")
        for chunk in needs.chunks(32):
            embeddings = encoder.encode_batch(chunk_paths)
            for ((id, _path), embedding) in chunk.zip(embeddings):
                db.upsert_embedding(id, "siglip2_base", embedding)
            emit Phase::Encode(processed, total)
        emit "encoder_run_summary" + "preprocessing_sample" diagnostics
    else:
        warn "SigLIP-2 image model missing"
```

### Text branch (semantic search path)

```
commands::semantic_fused::get_fused_semantic_search:
    // Phase 11d — text-image RRF fusion. For each enabled text-supporting
    // encoder (CLIP + SigLIP-2; DINOv2 is image-only, implicitly skipped),
    // encode the query, score against the matching image-side cosine cache,
    // then fuse via RRF.
    
    Siglip2TextEncoder::new(...) → encoder.encode(query) → Vec<f32> length 768
    └─► one of N ranked lists fed into cosine::rrf::reciprocal_rank_fusion
```

As of Phase 4 (commit `0f45344`) and then Phase 11d (text-image RRF fusion, commit `44ff366`), the text dispatch is wired. The legacy single-encoder `commands::semantic::semantic_search` IPC is still around as a fallback that takes a `text_encoder_id` parameter, but the frontend now routes `useSemanticSearch` through `get_fused_semantic_search` so every query naturally fuses across whatever text-capable encoders the user has enabled in Settings.

The SigLIP-2 text encoder is also pre-warmed during indexing (Phase 12d) — `indexing.rs` constructs it eagerly, and `Siglip2TextEncoder::new` runs `encode("warmup")` internally to pay ORT's first-inference initialisation cost off the user's interactive path.

## Implemented Outputs / Artifacts

- `<app_data_dir>/models/siglip2_vision.onnx` (~372 MB) loaded at image-encoder construction.
- `<app_data_dir>/models/siglip2_text.onnx` (~1.13 GB — large because the Gemma 256k vocab embedding matrix is huge) loaded at text-encoder construction.
- `<app_data_dir>/models/siglip2_tokenizer.json` (~34 MB) parsed at text-encoder construction.
- 768-d L2-normalised `f32` embeddings (image: per encode call; text: per query when wired).
- One storage destination per image: `embeddings(image_id, encoder_id="siglip2_base", embedding)` row.

## Known Issues / Active Risks

| Risk | Triggered by | Downstream impact |
|------|--------------|-------------------|
| Text-branch dispatch is not wired to the picker | User selects SigLIP-2 as Text→Image encoder in Settings | Today's runtime still uses CLIP. The picker shows an "experimental" warning. SigLIP-2 image embeddings are produced and stored; text queries against them just don't happen yet. |
| Aspect-preserving distortion | Very tall/wide images stretched to square | Subjects appear squashed in the encoder's view. Empirically not as harmful as it sounds — SigLIP was trained this way. Worse case: a portrait of a face stretched horizontally still encodes as "face" because the model learned the stretched-face distribution. |
| Text query > 64 tokens crashes the ONNX session | Long descriptive query | Hard-capped at 64 BEFORE the session call. Truncation is documented in the pipeline but the user gets no warning UI-side. |
| Text model is 1.13 GB — large download + slow load | First launch on a fresh install | Most of the size is Gemma's 256k vocab embedding matrix. Acceptable trade-off for the cross-lingual capability. |
| EP fallback is silent | Future CUDA attempt failing | CPU-only today; no EP probing. Forward-looking risk. |
| The MAP head's `pooler_output` is the joint-space vector — not `last_hidden_state[:, 0, :]` | Future swap to a different export that adds a CLS token | Trying to extract CLS would produce wrong-space embeddings. The verification pass confirmed `pooler_output` is the right output by reading HF's `SiglipModel.get_text_features` source. |

## Partial / In Progress

- (none — text-branch picker dispatch landed in Phase 4 on 2026-04-26; see `commands::semantic::semantic_search`'s `text_encoder_id` parameter and `TextEncoderState`'s two-slot shape (`encoder` for CLIP + `siglip2_encoder` for SigLIP-2). The frontend `useSemanticSearch` hook now reads `prefs.textEncoder` and threads it through.)

## Planned / Missing / Likely Changes

- **SigLIP-2 Large 384** as a quality-bump option. ~3× the size, materially better embeddings.
- **Text-image RRF fusion.** Encoding the same query through both CLIP and SigLIP-2 text encoders and fusing the results via RRF (the same Phase 5 algorithm used for image-image). Would need a new `get_fused_semantic_search` command. Low priority — the picker's instant-switch already lets users compare encoders manually.
- **Smart per-query encoder routing** — see the open concern in `notes/preprocessing-spatial-coverage.md`. SigLIP-2 is the natural target for color/scenery queries (no crop) and cross-lingual queries (256k Gemma vocab).

## Durable Notes / Discarded Approaches

- **Both branches use `pooler_output`, not `last_hidden_state[:, 0, :]`.** SigLIP uses MAP (Multi-head Attention Pooling) heads that produce projected outputs directly via `pooler_output`. CLS-token slicing would produce wrong-space embeddings. Verified by inspecting the ONNX graph and HF's `SiglipModel` source in the prior verification pass.
- **Text encoder takes ONLY `input_ids`, NO `attention_mask`.** The ONNX export inlined the position lookup as a fixed-length Slice; passing attention_mask makes session.run error. The position embedding table is fixed at 64 entries — sequences longer than 64 must be truncated before reaching the session.
- **Pad token is id 0 (`<pad>`), not the EOS that CLIP uses.** Gemma's SentencePiece has distinct pad/eos/bos/unk tokens, unlike OpenAI CLIP's "EOS doubles as pad" quirk.
- **No prompt prefix needed.** Some SigLIP variants benefit from "This is a photo of {X}." framing, but SigLIP-2's released processor does no templating, the tokenizer config has no prompt template, and HF's `AutoProcessor` example calls `processor(text=labels, …)` with raw labels. Confirmed by the verification pass.
- **The 256×256 input size is a SigLIP-2 specific** — SigLIP v1 used 224. The `-256-ONNX` suffix in the HF repo name is significant; the non-suffixed variant returned 401 in the prior verification pass.
- **Image preprocessing is exact-square stretch** because SigLIP-2 was trained that way. Aspect-preserving + crop (CLIP/DINOv2 style) would deviate from training-time geometry. The user-visible upside: every pixel of the original image contributes to the embedding (see `notes/preprocessing-spatial-coverage.md`).

## Obsolete / No Longer Relevant

The previous `Xenova/siglip2-base-patch16-224` URL (returned 401 — verified in the model-download research pass). The 224×224 input size from SigLIP v1.
