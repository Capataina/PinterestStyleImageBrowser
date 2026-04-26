# Area 2 — Fusion + search command surface

`src-tauri/src/commands/semantic_fused.rs` (NEW Phase 11d, 297 lines),
`src-tauri/src/commands/similarity.rs` (653 lines),
`src-tauri/src/commands/semantic.rs` (355 lines, legacy),
`src-tauri/src/similarity_and_semantic_search/cosine/rrf.rs` (309 lines).

## Findings

### D-SIM-1 — Legacy single-encoder similarity IPCs unreachable from the UI

- **Severity:** High
- **Category:** Dead Code
- **Location:**
  - Backend: `src-tauri/src/commands/similarity.rs:322-475`
    (`get_tiered_similar_images`),
    `src-tauri/src/commands/similarity.rs:477-653` (`get_similar_images`),
    plus their `tauri::generate_handler!` registrations in
    `src-tauri/src/lib.rs:466-467`.
  - Frontend: `src/services/images.ts:246-271` (`fetchSimilarImages`),
    `src/services/images.ts:273-284` (`fetchTieredSimilarImages`).
- **Confidence:** High (`grep` over `src/` shows zero non-test callers
  for `fetchSimilarImages` and `fetchTieredSimilarImages`; the only
  `useTieredSimilarImages` hook in `src/queries/useSimilarImages.ts`
  routes through `fetchFusedSimilarImages` instead).

**Current state.** Phase 5 + Phase 11d migrated every UI dispatch to
`get_fused_similar_images` (image-image) and
`get_fused_semantic_search` (text-image). The legacy
`get_similar_images`, `get_tiered_similar_images`, and the matching
service-layer functions are still registered, exported, and
documented, but no production code path calls them. The only callers
are unit tests in `src/services/services.test.ts`.

The vestigial state has a maintenance cost: every fusion change has
to be cross-checked against three IPC commands, three service
functions, and the two frontend hook signatures, even though only
one path is reachable.

**Proposed change.** Two-stage cleanup:

1. **Documentation (this audit).** Add a `// LEGACY — unreachable from
   the production UI as of Phase 11d. Kept as a fallback for the
   tiered-sampling diversity strategy described in
   `notes/fusion-architecture.md`. If the fallback is genuinely useful,
   wire a frontend toggle; otherwise, plan removal.` comment block at
   the head of each legacy IPC + service function.
2. **Removal (separate session).** Once the team confirms the fallback
   isn't needed, delete:
   - `commands/similarity.rs::get_similar_images` and
     `get_tiered_similar_images`,
   - `commands/semantic.rs::semantic_search` (see D-SEM-1 below),
   - the matching `tauri::generate_handler!` entries,
   - the matching `services/images.ts` exports,
   - the unit tests pinning their signatures,
   - and (in `cosine/index.rs`) `get_tiered_similar_images` (see
     D-COS-1) — its only Rust caller is the soon-to-be-deleted
     `commands::similarity::get_tiered_similar_images`.

**Justification.** Stage 1 is comment-only — zero behavioural change.
Stage 2 is "free" only if the legacy path is genuinely never needed
again; it's not in scope for this audit (which is identical-behaviour
findings).

**Expected benefit.** Stage 1 prevents the next reader from spending
effort on dead code paths. Stage 2 (separate session) removes ~600
lines of Rust and ~80 lines of TypeScript.

**Impact assessment.** Stage 1 is risk-free. Stage 2 is a behavioural
change in the sense that the legacy IPCs become unavailable — but
they're already unreachable from the UI, so the user can't tell.

---

### D-SEM-1 — Legacy single-encoder `semantic_search` IPC unreachable from the UI

- **Severity:** High
- **Category:** Dead Code
- **Location:** `src-tauri/src/commands/semantic.rs` (entire file is
  the `semantic_search` implementation + its helpers); registration
  in `src-tauri/src/lib.rs:469`; service in
  `src/services/images.ts:358-372`.
- **Confidence:** High (frontend grep shows `semanticSearch` is
  imported only by `src/services/services.test.ts:199, 213`. The
  production hook in `src/queries/useSemanticSearch.ts:30` calls
  `fetchFusedSemanticSearch` exclusively.)

**Current state.** Same situation as D-SIM-1 but for the text-image
direction. After Phase 11d landed `get_fused_semantic_search`, the
single-encoder dispatch became unreachable. The 355-line file +
helpers + tokenizer-output diagnostic + `record_clip_tokenizer_diagnostic`
function all run only in tests now.

**Proposed change.** Same two-stage approach as D-SIM-1. Stage 1 is a
header comment in `semantic.rs`; stage 2 deletes the file (and folds
`record_clip_tokenizer_diagnostic` into `commands/semantic_fused.rs`
if the tokenizer diagnostic is still wanted there).

Note: the constants `CLIP_TEXT_ENCODER_ID` and
`SIGLIP2_TEXT_ENCODER_ID` (lines 18-19 of `semantic.rs`) are
re-exported and consumed by `commands/semantic_fused.rs:42`. Move
those to a small constants module before deleting the file.

**Justification.** Stage 1 is comment-only.

**Expected benefit.** Same as D-SIM-1.

**Impact assessment.** Same as D-SIM-1.

---

### K-FUS-1 — `get_fused_semantic_search` returns empty when no text encoders enabled

- **Severity:** Medium
- **Category:** Known Issues and Active Risks
- **Location:** `src-tauri/src/commands/semantic_fused.rs:99-107`
- **Confidence:** High (diagnostic test
  `src-tauri/tests/audit_fusion_no_text_capable_encoders_diagnostic.rs`
  documents the contract)

**Current state.** When the user disables every text-capable encoder
(currently CLIP + SigLIP-2; DINOv2 is image-only), the IPC returns
`Ok(Vec::new())`. The user sees an empty result list with no message.
The `decide_enabled_write` IPC validator (`commands/encoders.rs:101-105`)
prevents the user from disabling *every* encoder, but they can still
end up with only DINOv2 enabled — which is a valid configuration for
image-image fusion but silently bricks text-image search.

**Proposed change.** Two ways forward:

1. **Surface a typed error.** Return
   `Err(ApiError::BadInput("No text-capable encoders are enabled. Enable CLIP or SigLIP-2 in Settings to use text search."))`
   instead of `Ok(Vec::new())`. The frontend's `formatApiError` already
   renders this kind of message, and the user gets actionable feedback.
2. **Block the toggle.** Extend `decide_enabled_write` to require at
   least one text-capable encoder *if* the UI exposes a text-search
   feature. This is a behavioural change to the toggle semantics —
   tighter spec, less flexible, but no surprise empty results.

The audit's identical-behaviour rule rules option 2 out unless the
user signs off on changing the toggle contract. Option 1 is a
behavioural change to the IPC contract (Ok → Err) but is the kind of
change that improves correctness — flag for the implementing engineer.

**Justification.** Currently `Ok(Vec::new())` is the same shape as a
genuine "no matches found" result, which is a confusing UX collision.

**Expected benefit.** Users get a clear "enable a text encoder" message
instead of "no results found".

**Impact assessment.** Behavioural change — flagged. Frontend would
need to handle the new ApiError variant, but `formatApiError` already
falls back to the message string so the worst case is the IPC error
showing as a toast.

---

### D-FUS-1 — `_force_pathbuf_used` is genuinely dead code with a misleading comment

- **Severity:** Low
- **Category:** Dead Code
- **Location:** `src-tauri/src/commands/semantic_fused.rs:291-297`

**Current state.** The function is `#[allow(dead_code)]` annotated and
the comment claims it exists to prevent an unused-import warning on
`PathBuf`. But `PathBuf` is *used* in this file via type inference at
line 200 (`p.to_string_lossy()` is called on `&PathBuf` from
`FusedItem::path`) — the import is genuinely needed for the function
signatures elsewhere. The dummy function does nothing.

**Proposed change.** Delete the function and its comment.

**Justification.** The comment claims the function prevents a warning;
the warning would not actually fire because `PathBuf` is used elsewhere
in the file. Identical behaviour.

**Expected benefit.** 7 lines of dead code removed, one misleading
comment gone.

**Impact assessment.** Zero — function is `#[allow(dead_code)]`,
nothing calls it.

---

### M-FUS-1 — Path-resolution + thumbnail enrichment block duplicated four times

- **Severity:** Medium
- **Category:** Modularisation
- **Location:**
  - `src-tauri/src/commands/semantic_fused.rs:178-205`
  - `src-tauri/src/commands/similarity.rs:241-275`
  - `src-tauri/src/commands/similarity.rs:394-422`
  - `src-tauri/src/commands/similarity.rs:557-597`
  - `src-tauri/src/commands/semantic.rs:116-144`

**Current state.** Five copies of the same `filter_map` over results
that:

1. Calls `resolve_image_id_for_cosine_path(&db, &path, all_images)`.
2. Records a miss path or a `thumb_misses += 1`.
3. Calls `db.get_image_thumbnail_info(id).ok().flatten()`.
4. Builds an `ImageSearchResult` with the destructured `(thumbnail_path,
   width, height)` triple.

The differences between the copies are tiny: the source iterator type
(`fused.iter()` vs `raw_results.iter().cloned()`), and the score field
(fused score vs cosine score).

**Proposed change.** Extract a single helper in `commands/mod.rs` (or
a new `commands/_search_helpers.rs`):

```rust
pub(crate) fn enrich_search_results<I, F>(
    db: &ImageDatabase,
    all_images: Option<&[ImageData]>,
    items: I,
    score_fn: F,
) -> (Vec<ImageSearchResult>, Vec<String>, u32)
where
    I: IntoIterator,
    F: Fn(&I::Item) -> (PathBuf, f32),
{ ... }
```

Returns `(results, resolution_misses, thumb_misses)` so the diagnostics
in each command keep working.

**Justification.** Behaviour preserved exactly — the helper is the
existing block lifted out. No new dependencies, no new abstraction
layer (the helper is `pub(crate)` and lives next to its callers).

**Expected benefit.** Five copies → one definition; five places to
update when the result shape changes → one. Roughly 80 lines of net
deduplication.

**Impact assessment.** Mechanical refactor. The audit notes it but
does not perform it (production source stays untouched per the
audit's Rule 3). Two of the five sites (D-SIM-1 and D-SEM-1 sites)
may be deleted before this refactor lands — schedule the dedup after
the dead-code cleanup so we don't refactor code that's about to be
removed.

---

### D-COS-1 — `cosine/index.rs::populate_from_db_for_encoder` legacy CLIP fallback is unreachable

- **Severity:** Medium
- **Category:** Dead Code
- **Location:** `src-tauri/src/similarity_and_semantic_search/cosine/index.rs:74-88`
- **Confidence:** Moderate (depends on the R8 + pipeline-version-3
  bump being installed; the pre-bump install path could in theory
  still hit it, but `notes/notes.md` confirms the wipe ran on
  2026-04-26)

**Current state.** When `get_all_embeddings_for("clip_vit_b_32")`
returns an empty vector, the code falls back to `get_all_embeddings()`
(reads the legacy `images.embedding` column). The Phase 12 pipeline
version bump wipes that column; R8 stops the encoder pipeline
re-populating it. So the legacy column is permanently empty for any
install that has been bumped past pipeline-version 3 (which is every
install as of 2026-04-26 per the upkeep notes).

The fallback now always returns the same empty vector that the
non-fallback path would have returned. It is reachable only on a
hypothetical install that:

(a) Has `images.embedding` data (i.e. ran some pre-Phase-12 build), AND
(b) Has not yet been bumped past pipeline-version 3 (i.e. has not
launched under the Phase 12 binary).

Both conditions are mutually exclusive in practice — bumping happens
on every launch under the new binary.

**Proposed change.** Remove the fallback block (lines 74-88), keeping
only the primary `db.get_all_embeddings_for(encoder_id)` call. The
empty-vector case becomes `Ok(empty vec)` directly with no second
attempt.

**Justification.** The fallback can no longer return a non-empty
vector for any installed-and-launched configuration. Deleting it
preserves observed behaviour.

**Expected benefit.** Removes 14 lines of unreachable code and the
attached `info!` log line that always runs but never finds anything.

**Impact assessment.** Behavioural change *only* on a hypothetical
install that hasn't yet been bumped past pipeline-version 3 — no such
install exists in the field.

**Confidence upgrade pathway.** A diagnostic test that exercises
`populate_from_db_for_encoder("clip_vit_b_32")` against a DB with the
legacy `images.embedding` populated and the new `embeddings` table
empty would lift this to `high`. Not written in this audit because
the post-bump scenario is the only one in the field.

---

## RRF math correctness

Read `src-tauri/src/similarity_and_semantic_search/cosine/rrf.rs` end-to-end:
the implementation matches the Cormack 2009 formula exactly.

- `k_rrf = 60` matches the canonical paper value.
- Aggregation by `HashMap<PathBuf, FusedItem>` with insertion-order
  preservation in the `per_encoder` evidence vector — correct.
- 1-indexed rank in the formula — correct (`rank0 + 1`).
- Empty list / zero top_n early returns — correct.
- Sort is `sort_unstable_by` with explicit NaN guard — fine; ties
  resolve to whatever the unstable sort picks, which is acknowledged
  in `single_encoder_preserves_order`'s test.

Six unit tests cover: empty input, top_n=0, single encoder preserves
order, consensus outranks lone winner, k_rrf sharpness ratio,
truncation. Coverage is comprehensive for the stateless math.

No findings here.

## `cosine/rrf.rs` Modularisation verdict

`leave-as-is`. Single concern (one pure function + two types + tests),
309 lines of which ~165 are tests. Production code is small.
