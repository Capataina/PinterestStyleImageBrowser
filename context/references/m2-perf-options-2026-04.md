# Performance optimisation options for M2 Apple Silicon

Research date: 2026-04-26. Stack baseline: Tauri 2 + Rust + `ort = 2.0.0-rc.10` + image-rs, three FP32 ONNX encoders (CLIP ViT-B/32 ~352 MB, DINOv2-base ~347 MB, SigLIP-2 base-256 ~372 MB), thumbnails via image-rs CatmullRom resize. Measured per-image cost on M2: CLIP 62 ms, SigLIP-2 189 ms, thumbnail 256 ms. CoreML EP confirmed broken for these graphs.

## TL;DR ranked recommendation list

| Option | Effort | Expected speedup | Quality cost | Confidence |
|---|---|---|---|---|
| 1. Drop-in FP16 ONNX vision encoders (Xenova / onnx-community) | XS — change download URL + dim assertion | ~1.5–2x CPU; halves disk + RAM | None measurable on cosine retrieval | High |
| 2. `fast_image_resize` (NEON) for the resize step | S — wrap in adapter from `image::DynamicImage` | 5–12x on the resize alone (433ms → 62ms RGB Lanczos3 published) | None (same filters) | High |
| 3. Native JPEG scaled decode (1/8, 1/4, 1/2) before resize | S — enable on `jpeg-decoder` or move to `zune-jpeg` | 4–8x on decode for large source JPEGs | None for thumbnails (downscale-then-resize) | High |
| 4. `mozjpeg-sys` / `turbojpeg` for JPEG decode (libjpeg-turbo SIMD) | M — C dep, build complexity | 2–3x on decode vs current pure-Rust path | None | High |
| 5. Replace SigLIP-2 vision encoder with INT8 ONNX | XS — file swap + recalibrate threshold | ~2–3x on the slowest encoder | ~1–4% recall drop typical for ViT INT8 PTQ | Medium |
| 6. Switch to MobileCLIP-S2 / MobileCLIP2-S2 instead of CLIP ViT-B/32 | M — re-export to ONNX, replumb embedding dim (probably 512) | 4–10x at the model level (Apple’s published latency) | **+3.4 IN-1k zero-shot acc** vs ViT-B/16 — quality goes up | Medium |
| 7. Decode-once buffer share between thumbnail and encoder steps | M — pipeline restructure | 1.3–1.5x on the JPEG-decode wall-clock (which is ~half of thumbnail step today) | None | High |
| 8. Tune `ort` intra/inter-op threads explicitly to M2 hybrid (4P+4E) | XS | 5–20% on encoder, possibly negative on power | None | Medium |
| 9. Migrate to `candle` with Metal backend | L — re-implement model wiring | Unknown — see Thread A §4; mixed signals | None if numerics match; quirks documented | Low |
| 10. MLX via `mlx-rs` (ANE-capable) | XL — experimental bindings, no CLIP/SigLIP loaders | Potentially large (ANE) | Unknown | Very low |

The first four items together plausibly take total session wall-clock from ~10 min down to ~3–4 min, with no model quality regression and only Rust-native dependencies. Items 5–7 are the next tier. Items 8–10 are research bets, not engineering wins.

---

## Thread A — encoder inference

### A1. `ort` ExecutionProviders that actually work on M2 in 2025/26

State of the art is grim and we should plan around it, not on it.

- **CoreML EP** is what you tried. The known failure mode (compiles ~54% of nodes, fails at first inference with code -1) is documented for graphs with dynamic shapes / mixed op coverage; ONNX Runtime upstream issue [#14212](https://github.com/microsoft/onnxruntime/issues/14212) explains the dynamic-shape trade-off and there are several 2025 reports of similar failures (e.g. [#26355 Parakeet CTC](https://github.com/microsoft/onnxruntime/issues/26355), [#28022 reflect-pad partition rounds](https://github.com/microsoft/onnxruntime/issues/28022), [#21170 scaling regression](https://github.com/microsoft/onnxruntime/issues/21170)). Two knobs that are sometimes the difference between “fails” and “works” and are worth one more pass before giving up entirely:
  - `ModelFormat=MLProgram` (default is the older `NeuralNetwork` format) — requires Core ML 5+ / macOS 12+, avoids implicit FP16 cast, and supports more ops. See [coreml_provider_factory.h](https://github.com/microsoft/onnxruntime/blob/main/include/onnxruntime/core/providers/coreml/coreml_provider_factory.h) and the writeup at [ONNX Runtime & CoreML May Silently Convert Your Model to FP16](https://ym2132.github.io/ONNX_MLProgram_NN_exploration).
  - `RequireStaticInputShapes=true` plus a wrapped graph that fixes batch dim to 1 (or to your fixed batch) — partitions the graph more cleanly and avoids the dynamic-shape failure mode. CLIP/DINOv2/SigLIP all accept fixed-batch inputs cleanly.

  If both still fail, treat CoreML as dead — we’re not the first to hit this and the upstream fix cadence is slow.
- **WebGPU EP** in upstream ONNX Runtime targets the ONNX Runtime Web build (Dawn-based), not the native Rust `ort`. There is no production-quality WebGPU EP exposed through `pyke/ort` for native macOS as of 2026-04. [ort docs](https://ort.pyke.io/perf/execution-providers) list it but the support matrix is thin.
- **Metal EP / dedicated** — does not exist in ONNX Runtime upstream. CoreML *is* the Metal-on-macOS path in ORT’s design.
- **`pyke/ort` macOS support trajectory**: the maintainer has explicitly stated little to no further macOS support work, x86_64-darwin is dropped, target raised to macOS 13.4 (release notes through rc.12, [pykeio/ort releases](https://github.com/pykeio/ort/releases)). This means **don’t expect upstream to fix CoreML for us**. It’s the strongest argument in this whole report for treating `ort` as a known-stable CPU runtime and seeking acceleration elsewhere (FP16, smaller models, different framework) rather than waiting for an EP fix.

**Verdict:** assume `ort`-on-M2 is CPU-only for the foreseeable future. Optimise on that assumption.

### A2. FP16 / INT8 / INT4 quantisation paths

**This is the single highest-leverage change in the whole report and it’s essentially free.**

Both Xenova and onnx-community publish a full quantisation matrix on HuggingFace for exactly the three encoders we use:

CLIP ViT-B/32 vision encoder ([Xenova/clip-vit-base-patch32](https://huggingface.co/Xenova/clip-vit-base-patch32/tree/main/onnx)):

| File | Size | Notes |
|---|---|---|
| `vision_model.onnx` | 352 MB | current FP32 baseline |
| `vision_model_fp16.onnx` | 176 MB | recommended start |
| `vision_model_int8.onnx` | 88.6 MB | aggressive |
| `vision_model_q4.onnx` | 63.6 MB | very aggressive |
| `vision_model_q4f16.onnx` | 53.3 MB | very aggressive |

DINOv2-base ([onnx-community/dinov2-base](https://huggingface.co/onnx-community/dinov2-base)) ships the same matrix: `model_fp16.onnx` 173 MB, `model_int8.onnx` 91 MB, `model_q4f16.onnx` 51.4 MB.

SigLIP-2 base-256 ([onnx-community/siglip2-base-patch16-256-ONNX](https://huggingface.co/onnx-community/siglip2-base-patch16-256-ONNX)) ships `vision_model_fp16.onnx` 186 MB, `vision_model_int8.onnx` 94.7 MB, `vision_model_q4f16.onnx` 54.7 MB.

**Realistic CPU speedup expectations on Apple Silicon:**
- FP16 on ARM CPU via ORT: typically 1.3–2x over FP32 because ARM v8.2-A FP16 vector ops are well-supported on M-series. This is the safe pick — quality is essentially identical.
- INT8 PTQ on ARM CPU: typically 2–4x over FP32 on ViT-class graphs; the OpenVINO benchmark on a similar retrieval task reports ~4x speedup with <0.16% accuracy delta on a careful PTQ ([OpenVINO blog](https://blog.openvino.ai/blog-posts/efficient-inference-and-quantization-of-cgd-for-image-retrieval-with-openvinotm-and-nncf)). Generic CLIP guidance: 1–5% accuracy drop, scenario-dependent ([Milvus quick-reference](https://milvus.io/ai-quick-reference/how-does-quantization-such-as-int8-quantization-or-using-float16-affect-the-accuracy-and-speed-of-sentence-transformer-embeddings-and-similarity-calculations)).
- INT4 / Q4 / BNB4: research from [arXiv 2509.21173](https://arxiv.org/html/2509.21173v1) explicitly warns of a “quantisation cliff” for naive 4-bit on CLIP visual encoders. Skip unless you are willing to A/B retrieval quality.

**Quality-cost note that matters for *this* app**: cosine search over learned embeddings is more forgiving than top-1 classification. Even if INT8 introduces ~3% recall@10 drop on COCO-style benchmarks, the user-facing experience of a Pinterest-style browser is unlikely to be perceptibly different. We should still validate empirically by storing both FP16 and INT8 embeddings on a 200-image golden set and comparing nearest-neighbour overlap, but this is a low-risk migration.

**Recommendation:** ship FP16 as the new default for all three encoders. Treat INT8 as a user-toggleable “fast mode” after we have a recall benchmark.

### A3. Smaller-model alternatives in the same families

[MobileCLIP / MobileCLIP2 from Apple](https://github.com/apple/ml-mobileclip) (CVPR 2024, TMLR Aug 2025) is the headline result. From their published table on iPhone Neural Engine:

| Model | Image params (M) | Image latency (ms) | IN-1k zero-shot (%) |
|---|---|---|---|
| CLIP ViT-B/32 (our current) | 87.8 | — | ~63 |
| MobileCLIP-S0 | 11.4 | 1.5 | 67.8 |
| MobileCLIP-S2 | 35.7 | 3.6 | 74.4 |
| MobileCLIP-B(LT) | 86.3 | 10.4 | 77.2 |
| **MobileCLIP2-S2** | 35.7 | 3.6 | 77.2 |
| MobileCLIP2-B | 86.3 | 10.4 | 79.4 |

Apple’s claim: MobileCLIP-S0 matches OpenAI ViT-B/16 quality at 4.8x speed, 2.8x smaller. MobileCLIP-S2 matches/beats SigLIP ViT-B/16 quality at 2.3x speed.

**Caveats — and this is exactly the kind of thing the prompt asked us to call out:**
- Apple’s ms-numbers are iPhone Neural Engine via CoreML, not M2 CPU via ORT. The relative ranking will hold; the absolute multiplier will shrink, possibly substantially.
- A user complaint on the [MobileCLIP-S2 OpenCLIP HF page](https://huggingface.co/apple/MobileCLIP-S2-OpenCLIP/discussions/3) reports MobileCLIP-S2’s image encoder *slower* than OpenCLIP ViT-B-32-256 on both CPU and GPU when run via the official PyTorch path — i.e. the ANE numbers don’t carry over. This deserves a real benchmark before betting on it.
- ONNX exports: Apple ships [coreml-mobileclip](https://huggingface.co/apple/coreml-mobileclip), not ONNX. We would need to re-export from the OpenCLIP variant or use [Ultralytics’ implementation](https://github.com/ultralytics/mobileclip) to produce ONNX. Non-zero work.
- Embedding dim differs (probably 512 for S-models). Existing stored embeddings would need re-indexing.

**Recommendation:** do not gate the perf wins on MobileCLIP. Keep it as a follow-up experiment after FP16 + fast_image_resize land; benchmark MobileCLIP2-S2 ONNX-exported on M2 CPU as the quality-up replacement for ViT-B/32.

### A4. Switching from `ort` to `candle` (HuggingFace’s Rust ML framework)

candle has first-party example implementations of [clip](https://github.com/huggingface/candle/tree/main/candle-examples/examples/clip), [dinov2](https://github.com/huggingface/candle/tree/main/candle-examples/examples/dinov2), [siglip](https://github.com/huggingface/candle/tree/main/candle-examples/examples/siglip), AND [mobileclip](https://github.com/huggingface/candle/tree/main/candle-examples/examples/mobileclip). All three of our encoder families have a path. There is also `candle-onnx` for loading ONNX directly.

**Metal backend reality check (this is where it gets ugly):**
- A user reported [qwen2-0.5b at 0.03 tok/s on Metal vs 20 tok/s on CPU](https://github.com/huggingface/candle/issues/1596) on an M2 8GB.
- Issue [#2659](https://github.com/huggingface/candle/issues/2659) (Dec 2024, open) documents Metal performance oscillating from 1.6 ms to 575 ms on the same op across iterations.
- Tracking issue [#2832 Metal issues with examples](https://github.com/huggingface/candle/issues/2832) is open with a list of broken examples.
- A separate comment (sentence-transformer translation): “about 5x slower than PyTorch”.
- The “25.9× faster than MLX” claim on [GarthDB/metal-candle](https://github.com/GarthDB/metal-candle/blob/main/BENCHMARKS.md) is for *LoRA forward passes only*, not encoder inference, and the same doc admits metal-candle is 5–13x slower than MLX for comparable ops.

**Verdict:** candle’s Metal backend is not in a state where I would bet a user-facing encoder pipeline on it without a substantial benchmark first. candle-CPU might be *competitive with* ort-CPU but is unlikely to beat it meaningfully. The honest answer is: this is a research bet, not an engineering win.

### A5. `burn` framework

Same shape as candle but more experimental for CV-encoder loading. [Issue tracel-ai/burn#3463](https://github.com/tracel-ai/burn/issues/3463) — “Slow metal performance” — suggests the same reality holds. No first-party CLIP/DINOv2/SigLIP implementations to plug in. Skip unless we want to write a model from scratch.

### A6. tract (Sonos)

CPU-only ONNX runtime in Rust. The maintainer comment in [discussion #688](https://github.com/sonos/tract/discussions/688) explicitly says GPU is mobile-GPU-shaped at best, and recommends `onnxruntime` for desktop GPU. Tract is well-optimised for ARM CPU but its scheduler is not obviously better than ort for ViT graphs. Worth benchmarking once if curious — not a primary recommendation.

### A7. ort tuning currently underused

M2 has 4 performance + 4 efficiency cores. Default ORT thread auto-detect typically picks all 8, which is wrong — efficiency cores hurt latency-bound workloads. Concrete actions:
- `with_intra_threads(4)` to pin to perf cores. Per [ONNX Runtime threading docs](https://onnxruntime.ai/docs/performance/tune-performance/threading.html): “Exponential-backoff mode is particularly beneficial on hybrid (P-core / E-core) and mobile platforms.” This is on by default in recent ORT but we should explicitly set it.
- `with_inter_threads(1)` for our use case (we batch sequentially, not parallel sub-graphs).
- `GraphOptimizationLevel::Level3` if not already set.
- Enable memory-pattern reuse and arena allocator on the session — both default-on for repeated inference but worth confirming.
- Batch size sweet spot: M2 unified memory means very large batches are tolerable; on ViT-B/32-class, batch 16–32 is typically the throughput plateau. We’re already at 32.

Realistic gain: 5–20%, possibly negative on battery. Worth doing because it’s a one-line change.

### A8. MLX via Rust

[mlx-rs](https://github.com/oxideai/mlx-rs) v0.25.x exists and is actively developed. ANE access is what would make this exciting. But: no published CLIP/DINOv2/SigLIP loaders, MNIST + Mistral are the canonical examples, and 57 open issues in late 2025. We would write the model wrappers ourselves and debug FFI. Re-evaluate in 6 months; not a 2026-now option.

### A9. Other things found

- `accelerate-src` plus a BLAS-accelerated build of ORT: ORT on macOS does not link Accelerate by default. If we built ORT from source against Accelerate, large matmuls would benefit from the AMX coprocessor. Not exposed by `pyke/ort` prebuilt — would require a custom build. Bookmark it.
- Preprocessing SIMD: the resize-to-224 + normalisation step before each encoder runs in ~1–3 ms per image on Rust; not a hotspot. fast_image_resize would also accelerate this for free.
- Per-image overhead: at 62 ms/image CLIP and 32-batch, dispatch overhead is negligible. Don’t chase it.

---

## Thread B — thumbnail pipeline

256 ms/image at 8-way rayon = wall-clock ~5s for 1842 images, but per-image cost matters during incremental indexing. The pipeline is decode-JPEG → resize → encode-JPEG. Of those three, decode and resize each dominate roughly equally for full-size DSLR JPEGs.

### B1. `fast_image_resize` (Cykooz, NEON-optimised)

**Single biggest win available in this thread.** Published numbers from the [official ARM64 benchmarks](https://github.com/Cykooz/fast_image_resize/blob/main/benchmarks-arm64.md) on Neoverse-N1 (Rust 1.87, fast_image_resize 5.1.4):

| Operation | `image` crate | `fast_image_resize` (NEON) | Speedup |
|---|---|---|---|
| Lanczos3 RGB8 4928×3279 → 852×567 | 433.80 ms | 62.16 ms | **7.0x** |
| Lanczos3 L8 same sizes | 258.32 ms | 20.15 ms | **12.8x** |
| CatmullRom RGB8 same sizes | 305.90 ms | 42.43 ms | **7.2x** |

These are Neoverse, not Apple Silicon — Apple’s NEON is wider and pairs with AMX, so M2 numbers should be at least as good. The crate has `bench-arm64.md`, full NEON pixel-format coverage (U8/U8x2/3/4, U16, F32), CatmullRom support, and a stable API. Current version 6.0.0 (2026-01-13), actively maintained ([CHANGELOG](https://github.com/Cykooz/fast_image_resize/blob/main/CHANGELOG.md)).

Integration: feature `image` enables `IntoImageView for image::DynamicImage`, so adapter code is short. Output goes back into image-rs for JPEG encode.

### B2. `mozjpeg-sys` / `turbojpeg` for JPEG decode/encode

Both are mature C bindings. [mozjpeg-rust (ImageOptim)](https://github.com/ImageOptim/mozjpeg-rust) updated Feb 2025; [turbojpeg crate](https://docs.rs/turbojpeg) at 1.4.0. libjpeg-turbo with NEON is the gold standard for JPEG decode speed on ARM. Realistic 2–3x decode speedup over image-rs’s pure-Rust `jpeg-decoder`, confirmed by [zune-jpeg’s own README](https://github.com/etemesi254/zune-image) calling itself “on par with libjpeg-turbo, far exceeding jpeg-decoder”.

Trade-off: C dependency, build complexity (cmake/automake), increases binary size, slightly trickier cross-compile. Acceptable for a Tauri desktop app.

### B3. `zune-jpeg` (etemesi254)

Pure-Rust JPEG decoder, 2x faster than image-rs jpeg-decoder per their [own benchmarks](https://etemesi254.github.io/assets/criterion/report/index.html), claims libjpeg-turbo parity. Active (2,277 commits as of Oct 2025). Caveats: AVX2/SSE4 documented, NEON SIMD is *not* explicitly mentioned in their docs — meaning on Apple Silicon some of the speedup may rely on autovectorisation rather than hand-tuned ARM intrinsics. Encoder is delegated to a separate crate ([jpeg-encoder](https://github.com/vstroebel/jpeg-encoder)).

image-rs upstream is [migrating jpeg-decoder → zune-jpeg](https://github.com/image-rs/image/issues/1845), so this transition is happening anyway.

**Recommendation:** if you want to stay pure-Rust, use zune-jpeg now (or wait for image-rs to swap default decoder). If you’ll accept a C dep, mozjpeg/turbojpeg is faster on ARM today.

### B4. JPEG scaled decode (1/2, 1/4, 1/8 at decode time)

This is huge for thumbnailing. A 6000×3376 → 400×400 thumbnail is wasting ~95% of the IDCT work in a full decode.

- **`jpeg-decoder`** exposes [`Decoder::scale()`](https://docs.rs/jpeg-decoder/latest/jpeg_decoder/struct.Decoder.html) supporting factors 1/8, 1/4, 1/2, 1. This is *real* native scaled decode — the documentation says “efficiently scales the image during decoding”. For 6000×3376 → 400×400, scale to 1/8 (750×422) and then resize to 400×400 — two orders of magnitude less IDCT work, and the final resize is a 2x downsample which is cheap.
- **`turbojpeg`** supports the same scaling factors plus more (1/16, 3/8, 5/8, 7/8, 9/8 …) via libjpeg’s native API.
- **`zune-jpeg`** — scaled decode not surfaced as a top-line feature in current docs.

This change alone — even keeping image-rs everywhere else — should cut thumbnail-step time roughly in half on typical large source JPEGs.

### B5. EXIF embedded thumbnail extraction

[kamadak-exif](https://docs.rs/kamadak-exif/) parses EXIF from JPEG cleanly. Camera and phone JPEGs near-universally embed a 160×120 thumbnail. The trade-offs:

- Pro: extraction is essentially free (no IDCT of the main image).
- Con: 160×120 is too small for a 400×400 masonry tile — visible quality loss, especially after upscaling.
- Con: not all images have it (screenshots, edited images, web-sourced).
- Con: EXIF orientation must still be respected.

**Recommendation:** worth implementing as a fast-path for the masonry “first paint” — extract EXIF thumbnail, display immediately, then queue a real thumbnail for replacement. Reduces perceived latency, doesn’t change throughput numbers.

### B6. Apple Image I/O via objc2

Possible. ImageIO on M-series uses hardware JPEG decode blocks. Bindings exist via `objc2`, but you’d be writing the wrapper yourself. Maintenance and cross-platform concerns are real (the Rust binary still needs to build and behave on Linux/Windows). I would not pursue this until B1+B4 have shipped and we’ve measured the new baseline.

### B7. AVIF/WebP storage instead of JPEG output

Smaller cached thumbnails (AVIF ~30% smaller than JPEG at same quality, WebP ~25%), faster *subsequent* loads from disk. Encode cost goes up significantly (AVIF encode in [ravif](https://crates.io/crates/ravif) is much slower than JPEG encode). Net loss for the initial thumbnail-generation pass; net win for steady-state cold loads later.

**Recommendation:** stay on JPEG for now. Revisit if cache disk usage becomes a complaint, or as a user-toggleable “small library” option.

### B8. Sharing decoded buffers between thumbnail and encoder steps

Each JPEG is currently decoded twice (once for thumbnail, once per encoder run = up to 4 times if all three encoders enabled). The *encoder* preprocessing actually wants 224×224 (CLIP), 224×224 (DINOv2), 256×256 (SigLIP-2) — all small, all derivable from the same scaled-decode source. This is structurally clean to do:

1. JPEG scaled decode to ~1/4 (e.g. 1500×845).
2. From that buffer, derive: thumbnail (resize to 400-on-long-edge), CLIP/DINOv2 input (resize to 224×224 + normalise), SigLIP-2 input (resize to 256×256 + normalise).
3. Run encoders + write thumbnail in parallel.

Saves at minimum one full JPEG decode per image; with three encoders on, saves three. Per-image savings rough estimate: 50–150 ms.

This is the highest-value structural change in the thumbnail pipeline and pairs perfectly with B1 + B4. It also implies a small refactor to the indexing pipeline (introduce a “decoded source” abstraction that fans out to thumbnail + encoder branches).

---

## What I’d actually try first, second, third

**First (quick wins, no model risk, days of work):**
1. Swap to FP16 ONNX vision encoders for all three models. Validate cosine recall@10 on a 200-image golden set vs FP32 baseline. Expect 1.5–2x encoder speedup, no quality regression.
2. Drop in `fast_image_resize` 6.x for the resize step. Expect 5–8x on the resize alone, immediately visible in thumbnail timings.
3. Set `with_intra_threads(4)` and `with_inter_threads(1)` on each ORT session. Confirm `GraphOptimizationLevel::Level3`.

**Second (medium work, larger payoff):**
4. Either (a) move JPEG decode to `zune-jpeg` for pure-Rust path, or (b) add `mozjpeg-sys` as a `--feature fast-jpeg` for users on macOS/Linux. Measure.
5. Adopt JPEG scaled decode (1/8 or 1/4 at decode time) for the thumbnail path. This is independent of (4) and pairs with either decoder.
6. Restructure the pipeline so each JPEG is decoded once at 1/4 and the resulting buffer fans out to thumbnail + encoders. This is the architectural change that unlocks the previous wins compounding.

**Third (research bets, validate before committing):**
7. INT8 ONNX vision encoders — A/B test, ship as user-toggleable “fast mode”.
8. MobileCLIP2-S2 as an optional CLIP replacement — requires re-export to ONNX and re-indexing of stored embeddings. Best as a “high-quality + fast” alternative encoder rather than a forced migration.
9. EXIF embedded thumbnail as masonry first-paint placeholder.

**Don’t do (unless we have spare research weeks):**
- candle/Metal migration — Metal backend not stable enough for a user-facing pipeline as of late 2025.
- mlx-rs — too early; no model loaders for our families.
- Custom ORT build linking Accelerate — high effort, uncertain payoff.
- AVIF/WebP cached thumbnails — wrong trade-off for initial-index throughput.

Realistic combined outcome of items 1–6: encoder phase ~5 min → ~2 min, thumbnail phase ~38s sequential / ~5s wall-clock → ~10s sequential / ~1.5s wall-clock. Total session ~10 min → ~4 min, with no model quality regression and only Rust-native deps (mozjpeg being the one C dep, optional behind a feature flag).

---

## Stale / dead ends

- **`pyke/ort` CoreML EP fixes** — maintainer has effectively de-supported macOS. Don’t wait on upstream.
- **WebGPU as an `ort` execution provider on native macOS** — exists in the docs but isn’t a real production target for the Rust crate today; it’s the Web build’s story.
- **`candle` Metal backend for production encoder inference** — multiple open issues with severe performance regressions (#1596, #2659, #2832). May be fine in a year; not now.
- **`burn` Metal backend** — same shape, less mature for our model families.
- **Naive INT4 quantisation of CLIP visual encoders** — “quantisation cliff” documented in [arXiv 2509.21173](https://arxiv.org/html/2509.21173v1). Either skip or use Q4F16 + careful PTQ.
- **`epeg`** — the gold-standard libjpeg-DCT-thumbnail library — has no maintained Rust binding. The C library itself has fallen out of active development.
- **mlx-rs** — bindings exist, MLX models for our families don’t. Wrong moment to invest.
- **Custom Accelerate-linked ORT build** — possible but unsupported by `pyke/ort` prebuilt; opens a maintenance burden disproportionate to the likely 10–20% gain.

---

## Open questions / unknowns

- **Real M2 numbers, not Neoverse**: the `fast_image_resize` benchmark is on Neoverse-N1. M2’s NEON + memory subsystem will give different (probably better) ratios. Need to bench in-project.
- **FP16 ORT speedup on ARM CPU specifically**: I’m extrapolating from the FP16-on-ARM body of work; the actual multiplier on `pyke/ort 2.0-rc.10` for our exact graphs is untested. First experiment should produce this number.
- **INT8 retrieval quality cost on *our* image distribution**: the published 1–5% accuracy numbers are on academic benchmarks (COCO, ImageNet). Personal-photo distributions could be more or less forgiving. Need a 200-image golden set.
- **MobileCLIP-S2 actual M2 CPU latency** from a clean ONNX export. The HF discussion thread suggests the iPhone-ANE numbers don’t carry to CPU — could be a non-win on M2 CPU even if it’s 5x faster on ANE.
- **Whether `RequireStaticInputShapes=true` + `ModelFormat=MLProgram` on the CoreML EP would unblock at least one of the three encoders**. One more clean attempt is cheap. If even one encoder works on CoreML, that single one might cleanly outperform anything CPU-only.
- **JPEG scaled decode quality vs `image-rs` end-result**: 1/8 → 400×400 introduces a slightly different sampling chain. Need a visual A/B before shipping.
- **Whether SigLIP-2’s ONNX is genuinely 3x slower than CLIP because of the patch-16 vs patch-32 difference (4x more patches → ~4x more attention compute), or because of an export quirk**: if it’s the former, FP16 + INT8 won’t close the gap; we need either MobileCLIP2 or SigLIP-2-base-256 from a different export.

Sources:
- [pyke/ort execution providers documentation](https://ort.pyke.io/perf/execution-providers)
- [pykeio/ort releases (macOS support drop)](https://github.com/pykeio/ort/releases)
- [ONNX Runtime CoreML EP documentation](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html)
- [microsoft/onnxruntime issue #14212 – dynamic shapes on CoreML EP](https://github.com/microsoft/onnxruntime/issues/14212)
- [microsoft/onnxruntime issue #28022 – reflect-pad partition rounds on Apple Silicon](https://github.com/microsoft/onnxruntime/issues/28022)
- [coreml_provider_factory.h – CoreML EP options](https://github.com/microsoft/onnxruntime/blob/main/include/onnxruntime/core/providers/coreml/coreml_provider_factory.h)
- [ONNX Runtime & CoreML May Silently Convert Your Model to FP16](https://ym2132.github.io/ONNX_MLProgram_NN_exploration)
- [Xenova/clip-vit-base-patch32 ONNX variants](https://huggingface.co/Xenova/clip-vit-base-patch32/tree/main/onnx)
- [onnx-community/dinov2-base](https://huggingface.co/onnx-community/dinov2-base)
- [onnx-community/siglip2-base-patch16-256-ONNX](https://huggingface.co/onnx-community/siglip2-base-patch16-256-ONNX)
- [Apple ml-mobileclip GitHub](https://github.com/apple/ml-mobileclip)
- [Apple MobileCLIP research page](https://machinelearning.apple.com/research/mobileclip)
- [MobileCLIP-S2 OpenCLIP HF speed discussion](https://huggingface.co/apple/MobileCLIP-S2-OpenCLIP/discussions/3)
- [Apple coreml-mobileclip on HuggingFace](https://huggingface.co/apple/coreml-mobileclip)
- [Reliability evaluation of CLIP under quantisation, arXiv 2509.21173](https://arxiv.org/html/2509.21173v1)
- [OpenVINO image-retrieval INT8 quantisation benchmark](https://blog.openvino.ai/blog-posts/efficient-inference-and-quantization-of-cgd-for-image-retrieval-with-openvinotm-and-nncf)
- [Milvus quantisation quick-reference](https://milvus.io/ai-quick-reference/how-does-quantization-such-as-int8-quantization-or-using-float16-affect-the-accuracy-and-speed-of-sentence-transformer-embeddings-and-similarity-calculations)
- [huggingface/candle](https://github.com/huggingface/candle)
- [candle Metal performance issue #1596](https://github.com/huggingface/candle/issues/1596)
- [candle Metal performance issue #2659](https://github.com/huggingface/candle/issues/2659)
- [candle Metal tracking issue #2832](https://github.com/huggingface/candle/issues/2832)
- [candle CLIP example](https://github.com/huggingface/candle/tree/main/candle-examples/examples/clip)
- [candle DINOv2 example](https://github.com/huggingface/candle/tree/main/candle-examples/examples/dinov2)
- [candle SigLIP example](https://github.com/huggingface/candle/tree/main/candle-examples/examples/siglip)
- [candle MobileCLIP example](https://github.com/huggingface/candle/tree/main/candle-examples/examples/mobileclip)
- [candle-onnx crate](https://crates.io/crates/candle-onnx)
- [tracel-ai/burn issue #3463 slow Metal performance](https://github.com/tracel-ai/burn/issues/3463)
- [sonos/tract GPU support discussion #688](https://github.com/sonos/tract/discussions/688)
- [oxideai/mlx-rs](https://github.com/oxideai/mlx-rs)
- [ONNX Runtime threading documentation](https://onnxruntime.ai/docs/performance/tune-performance/threading.html)
- [Cykooz/fast_image_resize](https://github.com/Cykooz/fast_image_resize)
- [fast_image_resize ARM64 benchmarks](https://github.com/Cykooz/fast_image_resize/blob/main/benchmarks-arm64.md)
- [fast_image_resize CHANGELOG (v6.0.0 2026-01-13)](https://github.com/Cykooz/fast_image_resize/blob/main/CHANGELOG.md)
- [etemesi254/zune-image](https://github.com/etemesi254/zune-image)
- [Shnatsel/zune-jpeg](https://github.com/Shnatsel/zune-jpeg)
- [image-rs/image issue #1845 – migrate decoder to zune-jpeg](https://github.com/image-rs/image/issues/1845)
- [jpeg-decoder Decoder::scale() docs](https://docs.rs/jpeg-decoder/latest/jpeg_decoder/struct.Decoder.html)
- [ImageOptim/mozjpeg-rust](https://github.com/ImageOptim/mozjpeg-rust)
- [turbojpeg crate docs](https://docs.rs/turbojpeg/)
- [kamadak-exif crate](https://docs.rs/kamadak-exif/latest/exif/)
- [accelerate-src crate](https://github.com/blas-lapack-rs/accelerate-src)
