# Area 4 — Database (`db/mod.rs`, `db/embeddings.rs`)

`src-tauri/src/db/mod.rs` (359 lines), `db/embeddings.rs` (647 lines).

The R1 + R2 + R3 + R9 perf bundle is well-implemented and tested. Most
findings here are about consistency drift between the `read_lock()`
convention and the actual call sites.

## Findings

### I-DB-1 — `get_embedding` uses writer connection on a foreground read path

See **I-ENC-4** in `area-3-encoders.md` — same finding, listed there
because the consumer (similarity command) is in the encoder area.

---

### D-DB-1 — `update_image_embedding` and `get_image_embedding` write/read the legacy column that is no longer populated

- **Severity:** Medium
- **Category:** Dead Code
- **Location:**
  - `src-tauri/src/db/embeddings.rs:12-38` (`update_image_embedding`)
  - `src-tauri/src/db/embeddings.rs:41-84` (`get_image_embedding`)
  - `src-tauri/src/db/embeddings.rs:97-144` (`get_all_embeddings`)
- **Confidence:** Moderate (depends on the
  pipeline-version-3 wipe being present in every install — the
  `notes/notes.md` upkeep-context note confirms it landed 2026-04-26)

**Current state.** Three legacy-column methods survive the R8 + R9
perf bundle:

- `update_image_embedding`: writes to `images.embedding`. Called only
  from `ClipImageEncoder::encode_all_images_in_database` (which is
  itself dead — see D-ENC-2) and from the test suite.
- `get_image_embedding`: reads from `images.embedding`. Called only
  from `commands/similarity.rs:521-525` and `:371-374` and `:61` —
  always inside `if encoder_id == "clip_vit_b_32" { db.get_image_embedding } else { db.get_embedding }`
  branches. Post-R8 + pipeline-bump, `images.embedding` is empty for
  every install, so the CLIP branch always returns
  `QueryReturnedNoRows` and the call effectively errors. The other
  branch (`db.get_embedding`) is the live path for every encoder
  including CLIP.
- `get_all_embeddings`: reads from `images.embedding`. Called only
  from `cosine/index.rs:79` inside the unreachable legacy fallback
  (see D-COS-1 in `area-2-fusion-and-search.md`).

**Proposed change.** Two-stage cleanup, same shape as D-SIM-1:

1. **Documentation.** Add a `// LEGACY — `images.embedding` is no
   longer populated after R8 + pipeline-version-3 wipe. This method
   exists only because its tests exist; consider removing both
   together.` comment header to each of the three methods.
2. **Removal (separate session).** Delete the three methods, the test
   modules in `db/embeddings.rs:350-537` that exercise them, and the
   `images.embedding` column itself via a Phase-13 schema migration.

**Justification.** Stage 1 is comment-only.

**Expected benefit.** Stage 1 prevents confusion. Stage 2 removes
~150 lines of method code, ~190 lines of tests, and one column from
the schema. The schema cleanup also drops one BLOB column from every
image row (small per-row but ~2 KB × N images saved on disk).

**Impact assessment.** Stage 2 is a real schema migration — needs
its own session and a corresponding pipeline-version bump. Stage 1
is risk-free.

---

### I-DB-2 — `get_images_without_embedding_for` uses writer connection but is called from a thread that already owns its writer

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src-tauri/src/db/embeddings.rs:317-335`

**Current state.** Called only from indexing (`indexing.rs:765` via
`run_clip_encoder_with_intra`'s
`database.get_images_without_embeddings()` and `:883` via
`run_trait_encoder`'s `database.get_images_without_embedding_for(encoder_id)`).
Each indexing thread already has its own `ImageDatabase` instance
(per `indexing.rs:658`), so the writer mutex is only contested by
the same thread.

Switching to `read_lock()` would route the read to the read-only
secondary connection, which (a) is consistent with the convention
and (b) frees the writer for the actual UPDATE/INSERT calls in the
encoder loop.

**Proposed change.** Switch
`let conn = self.connection.lock().unwrap();` to
`let conn = self.read_lock();` at line 321.

**Justification.** Identical observable behaviour. Tiny perf win
(reads no longer hold the writer mutex during the prepare/execute
cycle), but real for the encoder loop because each batch starts
with this query.

**Expected benefit.** Convention adherence; tiny perf win at batch
boundaries.

**Impact assessment.** None functionally. `:memory:` falls back to
writer per `read_lock()` semantics — tests continue to work.

---

### I-DB-3 — `count_embeddings_for` uses `read_lock()` but `get_pipeline_stats` (caller) probably uses writer elsewhere

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src-tauri/src/db/embeddings.rs:339-347`

**Current state.** `count_embeddings_for` correctly uses `read_lock()`
(line 342). Good — but it's worth verifying that the caller
(`db/images_query.rs::get_pipeline_stats`) uses `read_lock()` for
*every* SELECT in its body. If `get_pipeline_stats` mixes read and
write locks per-statement, the foreground stats poll picks up an
extra writer-mutex acquisition for no gain.

Not a finding in itself — flagged here because it's the kind of
drift that creeps in when a method gets new SELECTs added without
the convention being followed. The diagnostic test
`audit_db_read_lock_routing_diagnostic.rs` enumerates which methods
hit which connection.

**Proposed change.** None for `count_embeddings_for` itself; review
`get_pipeline_stats` and `images_query.rs` in a separate pass.

**Justification.** N/A.

**Expected benefit.** N/A.

**Impact assessment.** N/A.

---

### K-DB-1 — Multiple writer connections per process is acknowledged but worth a one-time link

- **Severity:** Low
- **Category:** Documentation Rot
- **Location:** `src-tauri/src/db/mod.rs:32-56`

**Current state.** The `ImageDatabase` struct's docstring (lines 32-56)
clearly documents the writer + reader split. It does not mention that
the indexing pipeline's encoder threads each open their own
`ImageDatabase` instance, which means at peak a real-world install
has 1 (main) + 3 × 2 (encoder threads) = 7 connections to the same
SQLite file. WAL handles this correctly (verified by the WebSearch
research <https://oldmoe.blog/2024/07/08/the-write-stuff-concurrent-write-transactions-in-sqlite/>),
but a future maintainer reading the docstring could be surprised.

**Proposed change.** Add a paragraph to the `ImageDatabase` docstring
mentioning the per-encoder-thread `ImageDatabase::new` pattern and
linking to `indexing.rs:653-664` for context.

**Justification.** Comment-only.

**Expected benefit.** Future maintainers understand the connection
fan-out.

**Impact assessment.** Comment-only.

---

### M-DB-1 — `db/embeddings.rs` test module is 290 lines (60% of the file)

- **Severity:** Low
- **Category:** Modularisation
- **Location:** `src-tauri/src/db/embeddings.rs:350-647`

**Current state.** Production code is ~250 lines; test module is
~290 lines. Within the test module, the dead-code methods (D-DB-1)
have ~190 lines of dedicated tests. Once the dead code is removed
(separate session per D-DB-1), the file shrinks naturally; until
then the size reflects the migration-in-progress nature of the file.

**Proposed change.** None — the file size is a symptom of the
dead-code situation, not a primary modularisation concern. Once
D-DB-1 is acted on, the file will shrink to ~250 lines on its own.

**Justification.** N/A.

---

## Modularisation verdict for `db/mod.rs` and `db/embeddings.rs`

`leave-as-is` for both. `db/mod.rs` is the orchestration entry point;
`db/embeddings.rs` is the embedding-storage concern (with the dead-
code situation that will shrink it naturally once D-DB-1 is acted on).
