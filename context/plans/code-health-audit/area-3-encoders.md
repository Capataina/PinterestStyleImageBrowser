# Area 3 — Encoder modules + ort_session

`src-tauri/src/similarity_and_semantic_search/encoder.rs` (370 lines),
`encoder_dinov2.rs` (215 lines), `encoder_siglip2.rs` (322 lines),
`encoder_text/encoder.rs` (310 lines), `ort_session.rs` (99 lines),
`preprocess.rs` (88 lines).

## Findings

### D-ENC-1 — `Siglip2ImageEncoder::new` and `Dinov2ImageEncoder::new` are dead

- **Severity:** Low
- **Category:** Dead Code
- **Location:**
  - `src-tauri/src/similarity_and_semantic_search/encoder_siglip2.rs:103-105`
  - `src-tauri/src/similarity_and_semantic_search/encoder_dinov2.rs:80-82`
- **Confidence:** High (`grep` for `Siglip2ImageEncoder::new(` and
  `Dinov2ImageEncoder::new(` across `src-tauri` returns zero matches)

**Current state.** Each of the three image encoders has a dual
constructor:

```rust
pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
    Self::new_with_intra(model_path, super::ort_session::DEFAULT_INTRA_THREADS)
}

pub fn new_with_intra(model_path: &Path, intra_threads: usize) -> Result<Self, ...>
```

For `ClipImageEncoder`, the integration tests in
`src-tauri/tests/similarity_integration_test.rs:30, 262` still call
`ClipImageEncoder::new(...)`, so its `new()` is live. For SigLIP-2
and DINOv2, no caller exists — both production code (always passes
through `new_with_intra`) and the test suite (no per-encoder
integration tests for SigLIP-2 / DINOv2).

**Proposed change.** Delete `Siglip2ImageEncoder::new` and
`Dinov2ImageEncoder::new`. Keep `ClipImageEncoder::new` because the
integration tests use it.

**Justification.** No callers; no behavioural change.

**Expected benefit.** Removes 6 lines per encoder (12 total) and one
fewer "which `new` should I call?" decision.

**Impact assessment.** None — confirmed via grep.

---

### D-ENC-2 — `ClipImageEncoder::encode_all_images_in_database` and `inspect_model` are dead

- **Severity:** Medium
- **Category:** Dead Code
- **Location:**
  - `src-tauri/src/similarity_and_semantic_search/encoder.rs:95-105`
    (`inspect_model`)
  - `src-tauri/src/similarity_and_semantic_search/encoder.rs:302-343`
    (`encode_all_images_in_database`)
  - `src-tauri/src/similarity_and_semantic_search/encoder_text/encoder.rs:161-171`
    (`inspect_model`, also dead in the text encoder)
  - `src-tauri/src/similarity_and_semantic_search/encoder_text/encoder.rs:237-288`
    (`encode_batch`, also dead in the text encoder)
- **Confidence:** High (grep returns zero callers in `src-tauri/src/`
  and `src-tauri/tests/`)

**Current state.**

- `inspect_model` is a debug helper that prints input/output names.
  No production caller and no test caller in either encoder.
- `ClipImageEncoder::encode_all_images_in_database` says in its
  docstring "Kept for back-compat with the pre-pipeline-thread era;
  the indexing pipeline now drives this loop directly so it can
  interleave with the thumbnail rayon pool. Retained because the
  test suite + smoke scripts still call it." Grep contradicts the
  comment: zero callers in tests, zero in `src/`. The function calls
  `db.update_image_embedding(...)` (line 337), which writes the
  legacy `images.embedding` column — also dead post-R8.
- `ClipTextEncoder::encode_batch` (lines 237-288) is the batch path
  for the text encoder. Production only ever encodes one query at a
  time (`encoder.encode(query)`). Zero callers.

**Proposed change.** Delete all four. The legacy column write
in `update_image_embedding` (`db/embeddings.rs:12-38`) is the only
remaining `update_image_embedding` caller; once
`encode_all_images_in_database` goes, that path may be removable too —
but check `db/embeddings.rs` test coverage first because the
embedding-storage tests still exercise it.

**Justification.** No callers, no behavioural change.

**Expected benefit.** ~80 lines deleted across two files. The
"back-compat" docstring will stop misleading future readers.

**Impact assessment.** None for the encoder methods. The DB-side
`update_image_embedding` is heavily tested (lines 350-537 of
`db/embeddings.rs`); those tests would need to be deleted alongside
the production method, or the method retained as a tested "low-level
write you can use if you want to write the legacy column directly"
helper.

---

### I-ENC-1 — `ClipImageEncoder::new_with_intra` accelerator path threads `intra_threads` only on macOS

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src-tauri/src/similarity_and_semantic_search/encoder.rs:78-93`

**Current state.** On non-macOS, `build_session_with_accel` ignores
its `intra_threads` parameter (`_intra_threads: usize` underscore
prefix on the binding) and builds a CUDA session with no thread
tuning. The macOS branch threads `intra_threads` through
`build_tuned_session_with_intra`. The function signature is the same
for both branches.

This is intentional per the comment ("intra_threads tuning is
irrelevant when CUDA is doing the work"), but it means a CUDA-only
test of indexing parallelism would silently use full default thread
counts even though the indexing pipeline carefully computed
`4 / num_enabled_encoders` and passed it in.

**Proposed change.** Either (a) add a comment to the non-macOS branch
referencing the macOS path so the asymmetry is documented from both
sides, or (b) pass `intra_threads` through to a CPU-fallback session
builder for the (rare) case where CUDA build fails. Option (a) is
the zero-behaviour-change path; option (b) is a small CUDA-side
correctness improvement.

**Justification.** Option (a) is comment-only. Option (b) is a tiny
behavioural change to the rare CPU-fallback-on-CUDA case.

**Expected benefit.** Future maintainers reading the non-macOS branch
won't wonder why `intra_threads` is unused.

**Impact assessment.** Comment-only edit (option a) is risk-free.

---

### D-ENC-3 — `Siglip2TextEncoder::new` warmup uses Trait method import inside the function body

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src-tauri/src/similarity_and_semantic_search/encoder_siglip2.rs:243-247`

**Current state.** Inside `Siglip2TextEncoder::new` (line 243), there's
a function-local `use` statement:

```rust
use crate::similarity_and_semantic_search::encoders::TextEncoder as TextEncoderTrait;
match encoder.encode("warmup") { ... }
```

The same trait is already imported at the top of the file (line 66:
`use super::encoders::{ImageEncoder, TextEncoder as TextEncoderTrait};`).
The function-local re-import is redundant.

The same pattern exists in `commands/semantic.rs:297` inside
`encode_with_siglip2`.

**Proposed change.** Delete the function-local `use` statement —
the file-level import is sufficient.

**Justification.** Identical resolution; less local noise.

**Expected benefit.** Removes 1 line per occurrence.

**Impact assessment.** None.

---

### I-ENC-4 — `commands/similarity.rs` reads embeddings from the writer connection on a foreground call

- **Severity:** Medium
- **Category:** Inconsistent Patterns
- **Location:**
  - `src-tauri/src/db/embeddings.rs:242-260` (`get_embedding`)
  - Callers (foreground IPC handlers):
    `src-tauri/src/commands/similarity.rs:184` (in
    `get_fused_similar_images`),
    `src-tauri/src/commands/similarity.rs:521-525` (in
    `get_similar_images`),
    `src-tauri/src/commands/similarity.rs:373` (in
    `get_tiered_similar_images`),
    `src-tauri/src/commands/similarity.rs:63` (in
    `run_cross_encoder_comparison` diagnostic)
- **Confidence:** High (diagnostic test
  `src-tauri/tests/audit_db_read_lock_routing_diagnostic.rs`
  enumerates the seven `self.connection.lock()` sites in
  `db/embeddings.rs`)

**Current state.** `notes/conventions.md` § "Read-only secondary
read_lock() for foreground SELECTs" sets the convention: "When adding
a new IPC SELECT, default to `read_lock()`." But `db.get_embedding`
uses `self.connection.lock()` (writer mutex). It's called from four
foreground IPC paths, including the new `get_fused_similar_images`
which is the production hot path.

The other affected DB methods that look like reads but use the writer
mutex:

- `db/embeddings.rs:41-84` — `get_image_embedding` (reads legacy
  column; legacy paths are dead per D-COS-1, but the method survives)
- `db/embeddings.rs:242-260` — `get_embedding` (reads
  per-encoder embedding for one image)
- `db/embeddings.rs:317-335` — `get_images_without_embedding_for`
  (called from indexing — already a writer thread)

The `get_embedding` route is the only foreground SELECT in this list;
the others are either dead (`get_image_embedding`) or always called
from a writer-owning thread (`get_images_without_embedding_for`).

**Proposed change.** Switch `get_embedding` to `self.read_lock()`. The
two-line change is the only one needed:

```rust
- let conn = self.connection.lock().unwrap();
+ let conn = self.read_lock();
```

**Justification.** SQLite WAL makes the read consistent without
serialising against the writer; the writer mutex was only being
acquired because the original method copied the
`update_image_embedding` shape. Identical observable behaviour from
the IPC's point of view.

**Expected benefit.** Foreground fusion calls no longer queue behind
in-flight encoder write batches. Per `db/mod.rs:52-56`, the
perf-1777212369 baseline showed that this kind of contention
contributed to 22 s outliers; switching `get_embedding` closes the
last foreground-read-on-writer hole that the audit could find.

**Impact assessment.** None functionally. `:memory:` test DBs fall
back to the writer connection automatically (per `read_lock()`'s
implementation).

---

### K-ORT-1 — Per-thread sessions risk thread_local memory leaks if threads outlive ORT runtime

- **Severity:** Low
- **Category:** Known Issues and Active Risks
- **Location:** `src-tauri/src/indexing.rs:653-707` (encoder thread spawn)
- **Confidence:** Moderate (based on
  <https://github.com/microsoft/onnxruntime/issues/15962> +
  <https://github.com/microsoft/onnxruntime/discussions/10107>)

**Current state.** Each encoder thread builds its own `ort::Session`
via `ClipImageEncoder::new_with_intra` /
`Siglip2ImageEncoder::new_with_intra` /
`Dinov2ImageEncoder::new_with_intra`, runs the encoding loop, then
exits (the thread is short-lived — ends when `run_encoder_phase`
joins).

Per the ONNX Runtime issue tracker, sessions held on threads that
outlive the ORT DLL/library can leak `thread_local` resources; CPU
EP with multi-thread session pools has a documented memory-leak
pattern when sessions are created and dropped per-iteration.

In this codebase the threads are short and the sessions outlive only
the encoding loop, then drop when the thread joins — the pattern
matches the leak shape ("create/drop per iteration") loosely if a
single indexing pass counts as one "iteration." Real-world impact is
likely small (one indexing pass per app launch + a few per
re-indexing event), but a long-lived app with many re-indexes could
accumulate.

**Proposed change.** Add a comment in `indexing.rs:639-642` (around
the `let mut handles: Vec<...>` declaration) noting the upstream
concern and pointing to a future enhancement: hoist the per-encoder
sessions to process-global Arcs so they're created once and reused
across indexing passes. Not done in this audit because it's a
behavioural reshape (session sharing across the thread-spawn boundary
needs Arc<Mutex<Session>> or `Send + Sync` semantics that ort 2.0-rc.10
documents but the code doesn't currently rely on).

**Justification.** Comment-only. Zero behavioural change.

**Expected benefit.** Future memory-pressure investigations have a
documented hypothesis to test against.

**Impact assessment.** Comment-only edit; no risk.

---

## `preprocess.rs` analysis

Read end-to-end. The `fast_resize_rgb8` helper has three fall-back
paths:

1. `FirImage::from_vec_u8` failure → fall back to `image::imageops::resize`.
2. `resizer.resize` failure → fall back to `image::imageops::resize`.
3. `RgbImage::from_raw` failure → fall back to `image::imageops::resize`.

Each fallback is logged with the encoder label so a future perf report
can identify which encoder's preprocessing degraded. The fallback
filter is `Lanczos3` to match the fast path — consistent.

Edge cases:

- `if sw == target_w && sh == target_h { return src.clone(); }` — short-
  circuits the no-op resize. Good.

No findings. The file is small, self-contained, and the fallback
discipline is exemplary. **`leave-as-is` modularisation verdict.**
