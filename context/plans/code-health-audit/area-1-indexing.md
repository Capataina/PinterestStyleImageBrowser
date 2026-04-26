# Area 1 — `indexing.rs` (parallel encoder phase, orchestration)

`src-tauri/src/indexing.rs` (1140 lines).

The file has had heavy churn from Phase 11c → 11e → 12b → 12c → 12d → 12e
on top of the Tier 1 + Tier 2 perf bundle. Comments and code are at
several different snapshots in time and disagree in places. The threading
model is sound and survives clippy + 125 lib tests; the findings here are
about hygiene, not correctness — except where a stale comment hides a
real contradiction (D-IDX-2).

## Findings

### D-IDX-1 — Dead arguments threaded through `run_encoder_phase`

- **Severity:** Low
- **Category:** Dead Code
- **Location:** `src-tauri/src/indexing.rs:573-579, 745`
- **Confidence:** High (verified by reading every body of the function;
  the audit test
  `src-tauri/tests/audit_indexing_parallel_encoder_diagnostic.rs`
  documents the inspection)

**Current state.** `run_encoder_phase` takes
`cosine_index: &Arc<Mutex<CosineIndex>>` and
`cosine_current_encoder: &Arc<Mutex<String>>` as parameters. The body
spawns one thread per enabled encoder — none of those threads use
either Arc. At the very end (line 745) the code consumes both bindings
with `let _ = (cosine_index, cosine_current_encoder);` and drops them.
The accompanying comment explains *why* they are unused (the old
priority-encoder hot-populate that used to live here was retired when
fusion landed — `FusionIndexState` lazy-populates per encoder instead),
but the bindings themselves remain on the function signature.

**Proposed change.** Drop both parameters from `run_encoder_phase`'s
signature and update the single caller in `run_pipeline_inner`
(lines 491-498) to omit them. The post-encoder safety-net populate at
`run_pipeline_inner:524-545` keeps using the same Arcs directly — it
needs them, the encoder phase does not.

**Justification.** Zero behavioural change: the parameters are
discarded inside the function. Removing them deletes ~6 lines of
parameter plumbing and lets the next reader trust the signature
(currently the function looks like it might mutate the cosine cache;
in fact it does not).

**Expected benefit.** Smaller signature, less misleading shape,
slightly cleaner call site. Inspection effort for the next person is
reduced by one indirection.

**Impact assessment.** No callers outside this file; private function.
The post-encoder safety-net populate at lines 524-545 still needs both
Arcs and is unaffected. Tests in `tests` module touch only
`IndexingState` + `IndexingProgress` — neither touches `run_encoder_phase`.

---

### D-IDX-2 — Module-level docstring describes the pre-multi-folder world

- **Severity:** Medium
- **Category:** Documentation Rot
- **Location:** `src-tauri/src/indexing.rs:1-27`
- **Confidence:** High

**Current state.** The `//!` docstring at the top of the file says the
two pipeline trigger paths are "App startup" and "`set_scan_root` IPC
command — the user picks a new folder; the DB is wiped and the same
pipeline is spawned to populate it." This describes the pre-Phase-6
single-scan-root architecture. The current reality (Phase 6 → 11) is:

1. App startup (still true).
2. `set_scan_root` (legacy — now wraps `add_root` semantics, doesn't wipe the DB).
3. `add_root` IPC (the real per-folder-add path).
4. `set_root_enabled` IPC (re-enabling triggers a pipeline).
5. `remove_root` IPC.
6. Filesystem watcher (`notify-debouncer-mini`) on any enabled root.

**Proposed change.** Rewrite the docstring's "Two trigger paths" block
to enumerate the current six paths (or summarise as "App startup and
any root-mutation IPC + the filesystem watcher"). Keep the rest of
the docstring (concurrency model, events) — those are still accurate.

**Justification.** Pure documentation update. Zero behavioural change.

**Expected benefit.** A new reader now gets a correct map of where
indexing is invoked from. Without this, they will hunt for
`set_scan_root` callers and miss the watcher and root-mutation paths.

**Impact assessment.** Comment-only edit; no risk.

---

### D-IDX-3 — `run_clip_encoder_with_intra` docstring still says "double-write"

- **Severity:** Medium
- **Category:** Documentation Rot
- **Location:** `src-tauri/src/indexing.rs:749-758`
- **Confidence:** High

**Current state.** The docstring above `run_clip_encoder_with_intra`
reads: "writes BOTH the legacy `images.embedding` column (kept for
backward-compat with semantic_search's existing reader) AND the new
`embeddings` table row keyed by encoder_id. This double-write goes
away in a future migration once everyone has re-indexed."

The body has already done that: line 818 passes `false` as the
`legacy_clip_too` argument to `upsert_embeddings_batch` (the audit
finding `R8` from the perf bundle). A separate inline comment at
lines 802-807 acknowledges the change ("R8 — legacy_clip_too = false").

The two comments contradict each other. The docstring is the one a
reader sees first.

**Proposed change.** Replace the "writes BOTH" docstring with: "Phase
12c — CLIP encoder loop. Writes only the new per-encoder embeddings
table; the legacy `images.embedding` double-write was retired in the
R8 perf bundle along with the pipeline-version-3 wipe of the legacy
column. Takes an explicit intra-thread count so the indexing pipeline
can size each parallel encoder's ORT pool. See
`ort_session.rs::build_tuned_session_with_intra`." Keep the second
half of the existing docstring about the explicit intra count — that
one is still correct.

**Justification.** Pure documentation update.

**Expected benefit.** Removes a misleading "we still write both
columns" claim that contradicts the code below it. A future reader
won't wonder why the legacy column write seems missing.

**Impact assessment.** Comment-only edit.

---

### I-IDX-1 — `super::similarity_and_semantic_search` instead of `crate::`

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src-tauri/src/indexing.rs:630`

**Current state.** Line 630 reads
`super::similarity_and_semantic_search::ort_session::DEFAULT_INTRA_THREADS`.
`super::` from `indexing.rs` (a top-level module under `lib`) resolves
to the crate root, so `super::similarity_and_semantic_search` works —
but every other reference inside `indexing.rs` uses
`crate::similarity_and_semantic_search::...` (e.g. lines 280-285, 412,
619-624, 675, 690-691). One stray `super::` is the odd one out.

**Proposed change.** Change line 630 to
`crate::similarity_and_semantic_search::ort_session::DEFAULT_INTRA_THREADS`.

**Justification.** Identical resolution — Rust accepts both and the
generated code is the same. Style consistency only.

**Expected benefit.** Future reader doesn't pause to wonder whether
`super::` is meaningful here.

**Impact assessment.** None.

---

### I-IDX-2 — Per-thread DB connection commit-amplification not documented

- **Severity:** Low
- **Category:** Documentation Rot
- **Location:** `src-tauri/src/indexing.rs:653-664`

**Current state.** Each spawned encoder thread calls
`ImageDatabase::new(&db_path)` and then `database.initialize()`. With
3 enabled encoders this opens 3 writer connections + 3 read-only
secondary connections to the same SQLite file (= 6 connections). The
inline comment says "well within SQLite's healthy concurrency
envelope" — accurate for SQLite (WebSearch confirms WAL serialises
writes at the file level even with multiple writer connections in one
process), but the comment doesn't mention that *initialize* runs
schema-create + migrations + every PRAGMA on each connection.

The schema-create is `CREATE TABLE IF NOT EXISTS` and idempotent, but
each connection still issues 8+ `pragma_update` calls in
`db/mod.rs::initialize`. With 3 encoder threads racing, that's ~24
PRAGMA statements compressed into the first ~50 ms after thread spawn.
SQLite handles this fine but it's worth a note: the per-thread
`initialize()` is paying for some redundant work that the main thread
already did.

**Proposed change.** Either (a) add a comment acknowledging the
redundant PRAGMA work and explaining why it is acceptable (briefly
documented in the existing comment but worth being explicit), or (b)
extract a `read_only_db_for_thread` helper that opens a connection
without re-running schema-create + PRAGMAs. Option (a) is the
zero-behaviour-change path.

**Justification.** No behavioural change for option (a). Option (b)
would be a tiny perf win at thread-spawn time but introduces new code
surface.

**Expected benefit.** Future readers understand why per-thread
`initialize()` is OK.

**Impact assessment.** Comment-only edit (option a) is risk-free.

---

### I-IDX-3 — Inline section comments out-of-order in thumbnail-phase

- **Severity:** Low
- **Category:** Documentation Rot
- **Location:** `src-tauri/src/indexing.rs:393-421`

**Current state.** The thumbnail phase opens with a long block comment
(lines 393-411) explaining the Phase-12b revert. Then the
`image_model_path` is computed at line 412 (which belongs to the
encoder phase that follows, not the thumbnail phase). Then the
thumbnail span starts at line 414. Then there's another nested comment
block (lines 415-420) about why thumbnails-then-encoders runs serially,
which restates roughly what the first comment block said.

The two comment blocks are saying the same thing in two different
spots, sandwiched around an unrelated `image_model_path` line.

**Proposed change.** Move the `image_model_path` computation (line 412)
to immediately before the encoder phase (line 491), and merge the two
comment blocks into one above `let _thumb_phase = ...`.

**Justification.** Pure code-movement + comment-merge; no behavioural
change.

**Expected benefit.** The thumbnail phase reads as one coherent unit
without the visual interruption of an unrelated path-computation +
duplicate explanation.

**Impact assessment.** Touches lines 393-421 only.

---

### K-IDX-1 — Single-flight pipeline does not coalesce *which* roots changed

- **Severity:** Low (existing known limitation worth surfacing)
- **Category:** Known Issues and Active Risks
- **Location:** `src-tauri/src/indexing.rs:127-141`

**Current state.** `try_spawn_pipeline` uses one global AtomicBool. If
the user adds root A, then immediately adds root B before the first
pipeline run finishes, the second `try_spawn_pipeline` call returns
`Err(AlreadyRunning)` and is silently dropped by the watcher. The
first pipeline does `db.list_roots()` at line 310 *before* it starts
processing, so it sees both roots and processes them — the silent drop
is not a functional bug for this case.

But it *would* be a functional bug if the second pipeline call carried
information the first one couldn't see. Today the only carrier is the
DB state (which is shared), so the silent drop is safe. If a future
change ever passes per-call data to `try_spawn_pipeline`, the silent
drop becomes a data-loss bug.

**Proposed change.** None right now — the current design is correct
for the current callers. Add a one-line comment near
`try_spawn_pipeline:127` warning future maintainers that
`AlreadyRunning` results are silently coalesced and any per-call
state would be lost.

**Justification.** Zero behavioural change.

**Expected benefit.** A note that prevents a future regression.

**Impact assessment.** Comment-only.

---

## Modularisation verdict for `indexing.rs`

`split-recommended` (M-IDX-1).

The file has three concerns that have grown to roughly equal size:

1. Pipeline orchestration (`try_spawn_pipeline`, `run_pipeline_inner`,
   `IndexingState`, `Phase`, `IndexingError`) — ~400 lines.
2. Encoder phase (`run_encoder_phase`,
   `run_clip_encoder_with_intra`, `run_trait_encoder`,
   `emit_preprocessing_sample`) — ~480 lines.
3. Event helpers (`emit`, `IndexingProgress`, the test module) —
   ~260 lines.

The three groups have minimal cross-coupling — concern (1) calls
into (2), concern (2) calls into (3) for emit, but concern (3) is a
leaf. A clean split:

```
src-tauri/src/indexing/
├── mod.rs          # public surface: IndexingState, IndexingError,
│                   # IndexingProgress, Phase, try_spawn_pipeline, emit
├── pipeline.rs     # run_pipeline_inner + the four phase-section blocks
└── encoder_phase.rs # run_encoder_phase + the two per-encoder loops
                    #  + emit_preprocessing_sample
```

Suggested as a future hygiene-focused session per the existing
`notes/notes.md` flag. Not done in this audit because the audit's
charter is identical-behaviour findings only; a module split is mostly
movement but introduces new file boundaries that would benefit from a
human eye on the seam.
