# Performance optimisation plan

## Header

- **Status:** Tier 1 + Tier 2 + Phase 4 + Phase 5 + Phase 6 + Phase 7 SHIPPED on 2026-04-26. R5 (FP16) deferred. App-support wiped — next launch re-encodes everything under the new pipeline.
- **Date:** 2026-04-26
- **Trigger:** lag during the 1842-image splash-arts indexing pass (two 22-second `get_images` UI freezes), plus mean SigLIP-2 encode at 252 ms/image and CLIP-text at 292 ms/call
- **Source materials this plan synthesises:**
  - `context/plans/performance-analysis.md` — diagnostic-first analysis of the 22 s stalls (frontend invalidation × Masonry × DB contention as the dominant chain)
  - `context/references/m2-perf-options-2026-04.md` — encoder + thumbnail acceleration research (FP16, fast_image_resize, JPEG scaled decode, candle/MLX state)
  - In-thread Agent B output — SQLite WAL contention root cause + fixes (read-only secondary connection + INSERT batching as the load-bearing pair)
  - In-thread Agent C output — `ort 2.0-rc.10` session tuning (Level3 + intra_threads(4) + dynamic_block_base + real-inference pre-warm)
  - Real perf reports under `~/Library/Application Support/com.ataca.image-browser/exports/perf-1777212369/`

## Executive summary

The lag is a **chain**, not a single bottleneck:

```
SigLIP-2 saturates CPU
    ↓
Encoder writes 1842 INSERTs (1 per row, no batching)
    ↓
Auto-checkpoint trigger blocks a COMMIT for seconds
    ↓
Foreground get_images waits behind the same Rust mutex
    ↓
Frontend invalidates ["images"] every 5s during encode
    ↓
Triggers full SELECT + 2000-item Masonry re-render
    ↓
Masonry re-render itself takes 86-404 ms even on a quiet system
    ↓
Visible UI freeze of 22 s
```

Each link in the chain is independently fixable. The **highest-leverage links to break first** are the frontend invalidation loop, the Masonry render cost, and the SQLite read/write separation — together they collapse the 22 s freeze to under 100 ms with ~150 lines of code change. Encoder-side wins (FP16 weights, ort tuning, fast_image_resize) come second and address the steady-state cost rather than the visible UX failure.

The skull-folder validation already proved the search pipeline itself is **correct**. This plan is purely about throughput and tail latency, not retrieval quality.

## Baseline (measured, perf-1777212369, 1842 splash arts on M2)

| Span / metric | n | mean | p50 | p95 | max | Verdict |
|---|---:|---:|---:|---:|---:|---|
| `siglip2.encode_image` | 1842 | 252 ms | 169 ms | **1.49 s** | **2.92 s** | Dominant cost; 6-10× tail |
| `clip.encode_image_batch` (batch=32) | 58 | 1.36 s | 1.29 s | 1.63 s | **3.66 s** | First batch eats warmup |
| `dinov2.encode_image` | 150 | 183 ms | 174 ms | 232 ms | 352 ms | Cleanest of the three |
| `thumbnail.generate` | 1842 | 19 ms | 12 ms | 43 ms | 97 ms | Lower priority than initially thought |
| `clip.preprocess_image` | 1842 | 11 ms | 11 ms | 12 ms | 86 ms | Small but adds up to 21 s total |
| `clip.encode_text` | 10 | **292 ms** | 300 ms | **628 ms** | 628 ms | Should be 5-15 ms — anomaly |
| `ipc.get_images` | 78 | 642 ms | 80 ms | 125 ms | **22.31 s** | Two 22 s outliers — the visible freeze |
| Masonry render (React Profiler) | 111 | 86 ms | — | — | **404 ms** | First-order UI bottleneck on its own |
| `set_priority_image_encoder` | 2/drawer-open | — | — | — | — | Fires twice per drawer open (StrictMode remount) |
| `cosine.populate_for_encoder` | 3 / session | 40 ms | — | — | 68 ms | Should be 1 per session — investigated, see §Done |

## What's already shipped (in working tree, uncommitted)

These are from Agents D and E during this session. Three coherent batches; each has its own tests passing.

### Batch 1 — Frontend dedup of `set_priority_image_encoder`

- `src/components/settings/EncoderSection.tsx` — module-level `lastPushedImageEncoder` cache that survives React 19 StrictMode mount/unmount/remount cycles (a `useRef` would be destroyed on unmount)
- `src-tauri/src/commands/encoders.rs` — backend short-circuit if `priority_image_encoder` value is unchanged, plus extracted `decide_priority_write` pure function for unit testing
- 4 new cargo tests + 1 new vitest test for the StrictMode-remount scenario
- Tests: 56 vitest + 114 cargo, all green

**Diagnosed in passing:** the 3× `cosine.populate_for_encoder` per session is *not* a bug. Breakdown: 1 real (priority hot-populate) + 2 diagnostic-only (cross-encoder comparison, gated behind `--profile`). The duplicate `set_priority_image_encoder` did not propagate into `invalidate()`. Closed without code change.

### Batch 2 — Frontend invalidation policy + Masonry virtualisation

- `src/hooks/useIndexingProgress.ts` — encode-phase invalidation removed entirely (encoders only change search-readiness, not visible grid); thumbnail-phase keeps the 5 s throttle (real visible change); `ready` invalidates exactly once per pipeline run via a `readyInvalidatedFor` ref keyed on the run's message; `scan`-phase reset re-arms for successive folder adds
- `src/components/Masonry.tsx` + `masonryPacking.ts` — viewport culling with ±800 px overscan, walks up to nearest scroll container, retains the selected hero unconditionally, mount-time perf marker for next-run comparison. Zero new dependencies (DIY rather than `react-virtuoso` / `react-window` because the custom shortest-column packing + 3-column hero promotion would not slot cleanly into a library's grid abstraction)
- 2 new vitest tests for `placement.height` in `masonryPacking.test.ts`
- Render cost should drop from O(N=2000) to O(visible≈30-100); target p95 well under 16.7 ms frame budget

### Batch 3 — `get_images` subspans for attribution

- `src-tauri/src/db/images_query.rs` — five `info_span!` subspans inside `get_images_with_thumbnails`:
  - `get_images.lock_wait` — Mutex acquire (the contention hypothesis)
  - `get_images.sql_prepare` — SQL parsing
  - `get_images.row_iter` — execute → cursor
  - `get_images.aggregate` — HashMap roll-up
  - `get_images.materialise` — `ImageData` mapping + sort
- Next perf run will attribute the (hopefully much smaller) remaining freeze to one of these — informs whether further DB tuning is needed

**Net for the 22 s freeze: Batches 2 + 3 should make it disappear without any backend changes.** Batch 2 stops re-rendering 2000 items every 5 s during encode; the freeze required (a) the freeze-causing work to be triggered and (b) the work to be expensive — Batch 2 removes (a), and even if (b) returns, the next perf report will pinpoint *which* substep matters via Batch 3.

## Recommended remaining work, ranked

Each item links to the originating research with a one-line summary of the evidence. **Effort estimates assume one focused session (1-4 hours). Impact estimates use the perf report's measured numbers as the floor — quoted ranges are conservative.**

### Tier 1 — high-confidence wins to land next

| # | Change | Effort | Expected impact | Source |
|---|---|---|---|---|
| **R1** | **Encoder INSERTs in `BEGIN IMMEDIATE` transactions of 256-500** | low (~30 lines in `indexing.rs`) | 10-100× insert throughput per [PDQ benchmark](https://www.pdq.com/blog/improving-bulk-insert-speed-in-sqlite-a-comparison-of-transactions/); compresses 7 min encoder write phase to <30 s; eliminates the per-row mutex/checkpoint churn that triggers the 22 s stalls | Agent B |
| **R2** | **Read-only secondary `Connection` for foreground `get_images` SELECTs** | low (~20 lines: open at startup, separate Mutex) | Foreground reads no longer wait behind the encoder mutex even when the encoder is mid-batch; converts the worst-case `get_images` from 22 s to ~50 ms | Agent B |
| **R3** | **Set `PRAGMA busy_timeout = 5000` + `wal_autocheckpoint = 0` + manual `wal_checkpoint(PASSIVE)` between batches + `journal_size_limit = 67_108_864`** | trivial (4 PRAGMAs in `db/mod.rs::initialize`, 1 call in encoder loop) | Caps WAL growth, makes checkpoint discipline explicit, prevents auto-checkpoint surprise stalls; safety net for R1+R2 | Agent B |
| **R4** | **`ort` session tuning: `Level3` + `intra_threads(4)` + `dynamic_block_base=4` + real-inference pre-warm** | low (~30 lines: helper `build_tuned_session(path)` + 2-line pre-warm change in `indexing.rs:259`) | Text encoder median 292 ms → 30-80 ms; image encoder p95/median ratio drops from 6-10× to 2-3×; max under 1 s. The 4 changes are independently documented Microsoft/pyke recommendations; combining them is the canonical M2 baseline | Agent C |

These four are the **minimum viable next PR**. Together they should:
- Kill the 22 s freeze (R1+R2+R3)
- Drop steady-state encoder cost by ~30-50% (R4)
- Keep the change surface small enough to reason about as one diff

### Tier 2 — meaningful wins, slightly bigger surface

| # | Change | Effort | Expected impact | Source |
|---|---|---|---|---|
| **R5** | **FP16 ONNX weights for all three image encoders** | low (URL swap in `model_download.rs` + verify input/output shapes unchanged + re-warm caches) | 1.5-2× CPU speedup, half the disk size, no measurable quality cost. Xenova/onnx-community publish FP16 variants at the exact paths we already use | Agent A |
| **R6** | **`fast_image_resize` 6.x for thumbnail resize step** | low-med (replace `image::imageops::resize` calls in `thumbnail/generator.rs` + dependency add) | 7-13× speedup on the exact RGB8 + Lanczos3 case we use, per [published ARM64 benchmark](https://github.com/cykooz/fast_image_resize#bench) (433 ms → 62 ms on 4928×3279). Single biggest thumbnail-pipeline lever even though thumbnails are no longer the dominant cost | Agent A |
| **R7** | **JPEG scaled decode (1/8 or 1/4) for thumbnail step** | low (`jpeg_decoder::Decoder::scale()` is real native scaled decode, not post-decode resize) | For 6000×3376 → 400×400 thumbnails, saves ~95% of IDCT work. Stack with R6 — scaled-decode produces a smaller buffer, then `fast_image_resize` does the final shrink | Agent A |
| **R8** | **Drop legacy `images.embedding` column write path** | low (audit call sites, delete the double-write in `run_clip_encoder`, schema-level cleanup later) | Removes per-CLIP-image extra UPDATE that dirties the wide `images` row → reduces page churn that exacerbates checkpoint stalls. Free perf | Agent B |
| **R9** | **Index audit + add `images_root_orphaned ON images(root_id, orphaned)`** if `EXPLAIN QUERY PLAN` shows SCAN | trivial | Mostly a safety check; quiet improvement if any SELECT was doing a full scan post-Tier-1 | Agent B |

### Tier 3 — structural changes; commit only if Tier 1+2 isn't enough

| # | Change | Effort | Expected impact | Source |
|---|---|---|---|---|
| **R10** | **Foreground/background encoder split**: thumbnails + ONE primary encoder run as foreground (interactable as soon as done); the other two run as background enrichment | medium (~150 lines: phase-aware pipeline, frontend phase indicator, persistence of "primary" choice) | Drops time-to-interactable from ~30 min to ~3 min on a 2000-image fresh add. The user gets fast first usability while richer embeddings cook in the background | `context/plans/performance-analysis.md` recommended fix #2 |
| **R11** | **Decode-once-fan-out for image encoders + thumbnail** | medium-high (~80 lines: shared decoded buffer, refactor preprocess functions to take `&Image` rather than `&Path`) | Same JPEG currently decoded 2-4× per image (thumbnail + each encoder). Saves 50-150 ms per image cumulative | Agent A |
| **R12** | **Disable `ort` per-session thread pools, use one global pool** | medium (~30 lines, requires verifying `with_disable_per_session_threads` exists in rc.10) | Kills 32-thread oversubscription when text + image encoders coexist; cleaner architecture for any future second-encoder-during-search work | Agent C |
| **R13** | **`deadpool-sqlite` connection pool (1 writer + N readers)** | medium (~50 lines, every call site changes shape) | Cleaner than R2's manual second connection; gives async-native ergonomics. Only do this if R1+R2 prove insufficient or if a future feature needs more concurrent reads | Agent B |

### Tier 4 — research / experimentation, bigger commitment

| # | Change | Effort | Expected impact | Source |
|---|---|---|---|---|
| **R14** | **INT8 ONNX variants** (CLIP, DINOv2, SigLIP-2 if available) | high (find or quantise yourself, validate retrieval quality on a labelled set, add benchmark harness) | Additional 1.5-2× on top of FP16 if quality holds. Watch the [int4 quantisation cliff on CLIP visual encoders](https://arxiv.org/abs/2509.21173) — int8 is generally safe, int4 is not without careful work | Agent A |
| **R15** | **MobileCLIP-S2 evaluation** | high (find ONNX export, benchmark on M2 *CPU* not ANE — published numbers are ANE-only) | Could be a 2-3× speedup if the published ANE numbers approximately translate to M2 CPU. [HF discussion](https://huggingface.co/apple/MobileCLIP-S2-OpenCLIP/discussions) reports it slower than ViT-B-32-256 on CPU though, so benchmark before committing | Agent A |
| **R16** | **One last CoreML EP attempt with `ModelFormat=MLProgram` + `RequireStaticInputShapes=true` + fixed batch dim** | medium | Sometimes flips a "doesn't work" CoreML graph to "works." If it does, ANE acceleration is order-of-magnitude faster. If it doesn't, abandon CoreML permanently | Agent A |

### Explicit dead ends (saved you the dig)

- **`pyke/ort` maintainer has de-supported macOS** going forward — don't wait for upstream fixes
- **`candle` Metal backend isn't production-ready** in early 2026 — issues #1596/#2659/#2832 show 1.6 ms ↔ 575 ms variance on the same op, sentence-transformer 5× slower than PyTorch
- **`mlx-rs` has no CLIP/DINOv2/SigLIP loaders** — too early
- **`PRAGMA mmap_size = huge` on macOS** — SQLite's own team documents no measurable benefit on Darwin
- **`PRAGMA journal_mode = TRUNCATE`** — you'd lose reader/writer concurrency entirely; worse for our workload
- **Adding more concurrent writer connections** — SQLite serialises at the WAL layer; more writers = more lock-handoff overhead with no throughput gain
- **IoBinding for CPU-only ort sessions** — explicitly unsuitable per ort docs; designed for GPU device-transfer avoidance
- **`ort with_parallel_execution(true)`** — explicitly hurts performance on graphs without many branches; transformers don't have many
- **Increasing `intra_threads` to 8 on M2** — collapses P-core frequency to E-core frequency due to mixed-cluster activation

## Implementation order (concrete next sessions)

### Session A (1-2 hours): commit what's already done

1. Review the three batches in working tree (Batch 1 IPC dedup, Batch 2 invalidation+Masonry, Batch 3 subspans)
2. Three commits — one per batch, message references this plan + perf-analysis.md
3. Re-run the 1842-image profile after restart; compare `ipc.get_images` max, Masonry render p95, total session time
4. **Decision gate:** if `ipc.get_images` max is now under 1 s, the 22 s freeze is fixed and Tier 1 R1-R3 become *nice to have* rather than *required*. The new perf report's `get_images.*` subspans tell you which substep deserves the next round of work.

### Session B (2-3 hours): Tier 1 — the 4-fix bundle

If the post-Batch-2 perf run still shows multi-second `get_images` outliers (or you want belt-and-braces), land R1+R2+R3+R4 as one PR. Expected outcome: encoder-write phase drops from 7 min to <30 s; encoder-inference tail drops from 1.5 s p95 to 200-300 ms p95; foreground reads no longer correlate with encoder activity at all.

### Session C (1-2 hours): Tier 2 — image-pipeline acceleration

R5 (FP16) + R6 (fast_image_resize) + R7 (scaled JPEG decode). Each is independently shippable. Drop them in any order; FP16 is the smallest diff.

### Sessions D+ (judgement call): Tier 3 if needed

The foreground/background encoder split (R10) is the cleanest UX win for fresh-folder adds. The decode-once fan-out (R11) is a real perf win but a bigger refactor.

## Success criteria (re-measure after each session)

The same 1842-image splash-art folder add should produce a perf report where:

| Metric | Current | Target after Session A | Target after Session B | Target after Session C |
|---|---:|---:|---:|---:|
| `ipc.get_images` max | 22.31 s | <1 s | <100 ms | <100 ms |
| `ipc.get_images` p95 | 125 ms | <125 ms | <50 ms | <50 ms |
| Masonry render p95 | 86 ms | <16 ms | <16 ms | <16 ms |
| Masonry render max | 404 ms | <50 ms | <50 ms | <50 ms |
| Renders >100 ms | 39 | 0 | 0 | 0 |
| `siglip2.encode_image` p95 | 1.49 s | 1.49 s (unchanged) | 300-500 ms | 200-300 ms |
| `clip.encode_text` mean | 292 ms | 292 ms (unchanged) | 30-80 ms | 30-80 ms |
| Total time-to-interactable | ~30 min | ~7 min | ~3 min | ~3 min |
| Total time-to-all-encoders-done | ~30 min | ~12 min | ~7 min | ~5 min |

## Assumptions needing stronger evidence

- **The 22 s `ipc.get_images` outliers correlate with encoder write bursts.** Strongly circumstantial (both stalls land in the SigLIP-2 phase) but Batch 3's subspans will *prove* whether the dominant substep is `lock_wait` (confirms contention) or something else. **What would refute this:** if `lock_wait` is sub-10 ms during a stall but `aggregate` or `materialise` is multi-second, the bottleneck is in-memory work and the read-only-second-connection fix becomes irrelevant.
- **FP16 quality cost is "no measurable degradation."** True for general-purpose retrieval benchmarks (COCO, Flickr30k). May not hold for narrow-domain corpora like splash arts where pairwise distances are already bunched. **Validation:** run the existing `pairwise_distance_distribution` diagnostic before/after FP16 swap; if the histogram noticeably tightens, FP16 is hurting more than expected.
- **Masonry virtualisation works correctly with hero promotion.** The DIY culling explicitly retains the selected hero, but the across-3-columns promotion is a special case — needs visual verification that scrolling past a promoted hero doesn't show ghost layout.
- **`ort` `with_disable_per_session_threads` is exposed in 2.0-rc.10's Rust API.** Documented in the C++/Java APIs; not 100% confirmed for the Rust binding in rc.10. Verify before committing R12.

## Failure modes and counter-scenarios

- **Tier 1 R1+R2+R3 lands and stalls return after a few weeks.** Means the WAL is growing again because the manual checkpoint discipline broke (someone removed the `wal_checkpoint(PASSIVE)` call, or a new code path bypasses the encoder-batch boundary). Mitigation: keep `wal_autocheckpoint = 200` as a backstop even with manual checkpointing — slightly more total fsyncs but pricelessly safe.
- **R6 (`fast_image_resize`) helps thumbnails but not the 22 s freeze.** Already known — thumbnails aren't on the 22 s freeze path. Land it for steady-state cost reduction, not for fixing the visible bug. If post-Batch-2 the user no longer experiences any visible freeze, R6 becomes a "make indexing twice as fast" rather than a "fix the bug" change.
- **R10 (foreground/background split) leads to "incomplete" feeling indexing.** Users may complain that "it says ready but search isn't using SigLIP-2 yet." Mitigation: expose the per-encoder progress in the Settings drawer (already shipped) and add a tooltip on the indexing pill explaining what's done vs cooking.
- **CoreML EP attempt R16 actually works on one encoder but not the others.** Real possibility — partition acceptance varies by graph. Be ready to ship a per-encoder EP choice rather than a global one. The encoder picker UI is the right place to surface "this encoder is using ANE" if it ever lands.

## What's explicitly NOT in scope for this plan

- Retrieval *quality* improvements (multi-encoder rank fusion, illustration-tuned CLIP variants, higher-res SigLIP-2-512, smart per-query routing) — covered separately by `notes/preprocessing-spatial-coverage.md` and `context/enhancements/recommendations/`. Those are quality wins; this plan is throughput.
- Switching to `candle` or `burn` — Agent A's research concluded the Metal backends aren't production-ready in early 2026.
- HNSW or any approximate-nearest-neighbour backend — irrelevant at <50k images per the earlier discussion; revisit when the library scales past that. Already in `context/enhancements/recommendations/02-hnsw-index-behind-trait.md`.
- Pipeline parallelism #74 — covered separately by `plans/pipeline-parallelism-and-stats-ui.md`. R10 (foreground/background split) is adjacent but distinct.
- The text encoder dispatch beyond CLIP (SigLIP-2 text) gap — known and tracked in `notes.md`; this plan optimises whatever encoder is actually running, not the picker dispatch.

## Open obligations / breadcrumbs for the next session

- **Verify the in-tree changes still build cleanly together** before committing — three batches landed independently, each was verified in isolation but not as a stack. `cargo check` + `cargo test --lib` + `npx tsc --noEmit` + `npm test -- --run` after rebasing/squashing.
- **Re-run a perf session after each commit** — the on-exit report now includes the `get_images.*` subspans (Batch 3) which will tell us whether further DB work is warranted.
- **The `model_download.rs` URL list is hardcoded** — when swapping to FP16 (R5), grep for the model URL constants (`CLIP_VISION_URL`, `CLIP_TEXT_URL`, etc.) to find every site that needs updating.
- **The `load_from_disk_if_fresh` cosine cache** is keyed by DB mtime; switching to FP16 will produce a different embedding distribution that invalidates cached cosine results. Bump `CURRENT_PIPELINE_VERSION` in `db/schema_migrations.rs` from 2 → 3 to wipe stale embeddings on first launch under the new code, same pattern used for the preprocessing fix.
