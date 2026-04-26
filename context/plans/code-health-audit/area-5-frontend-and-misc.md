# Area 5 — Frontend dispatch + miscellany

Covers the frontend search-routing migration (`useSimilarImages.ts`,
`useSemanticSearch.ts`, `services/images.ts`,
`components/settings/EncoderSection.tsx`), `perf.rs` /
`perf_report.rs` Phase 7 sampler, `settings.rs` `priority_image_encoder`
deprecation contradiction, dependency audit, and the general sweep for
stale `R<n>` comments and TODOs.

## Findings

### D-FE-1 — Frontend service exports are dead

- **Severity:** Medium
- **Category:** Dead Code
- **Location:**
  - `src/services/images.ts:246-271` (`fetchSimilarImages`)
  - `src/services/images.ts:273-284` (`fetchTieredSimilarImages`)
  - `src/services/images.ts:358-372` (`semanticSearch`)
- **Confidence:** High (`grep -r 'fetchSimilarImages\|fetchTieredSimilarImages\|semanticSearch' src/`
  returns matches only in `services.test.ts`)

**Current state.** Same situation as D-SIM-1 + D-SEM-1 from the backend
side: the service-layer wrappers for the legacy single-encoder IPCs
are exported but only the unit tests in `src/services/services.test.ts`
import them. Production hooks
(`useSimilarImages.ts::useTieredSimilarImages`,
`useSemanticSearch.ts::useSemanticSearch`) call the fused variants
exclusively.

**Proposed change.** Same two-stage approach as D-SIM-1 — header
comment now, deletion in a coordinated cleanup session that also
drops the matching backend IPCs.

**Justification.** Stage 1 is comment-only.

**Expected benefit.** Removes ~80 lines of TypeScript once stage 2
lands.

**Impact assessment.** Same as D-SIM-1 — flagged.

---

### D-FE-2 — `getThumbnailPath` helper is dead in production

- **Severity:** Low
- **Category:** Dead Code
- **Location:** `src/services/images.ts:171-173`

**Current state.** `getThumbnailPath(imageId)` is called only from the
`mapImageSearchResult` helper at line 238 as a fallback when
`res.thumbnail_path` is undefined. Since R6 (the perf bundle that
ensured thumbnail metadata is always returned in the IPC payload) and
the Phase 12 migration that re-thumbnails everything, `thumbnail_path`
is always present for any image rendered through `mapImageSearchResult`.
The fallback path is functionally dead.

The audit can't be 100% sure without an integration test exercising
`mapImageSearchResult` with `thumbnail_path: undefined`, so this is
moderate confidence.

**Proposed change.** None for now — the fallback exists for
defensive correctness against legacy data that might not have a
thumbnail path. The cost is minimal (3 lines).

**Justification.** Defensive code with low maintenance cost; safer
to keep.

**Expected benefit.** N/A.

**Impact assessment.** N/A.

---

### I-FE-1 — `useTieredSimilarImages` hook name doesn't reflect the fusion implementation

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src/queries/useSimilarImages.ts:22-32`

**Current state.** The hook is named `useTieredSimilarImages` but
internally calls `fetchFusedSimilarImages` (Phase 5 fusion). The
docstring (lines 5-21) explains that the name is preserved for
caller stability. The `encoderId` argument is documented as a hint
only ("fusion uses every available encoder regardless").

This is a deliberate trade-off documented in
`notes/fusion-architecture.md` ("Why route through
`useTieredSimilarImages` rather than introduce
`useFusedSimilarImages`. Renaming the hook would force a wave of
import updates"). Net result: the production codebase has *one*
similarity hook with a misleading name.

**Proposed change.** Two ways forward:

1. **Status quo.** The docstring already explains it. Leave alone.
2. **Rename via a thin wrapper.** Add a new
   `useFusedSimilarImages = useTieredSimilarImages` re-export in the
   same file; deprecate `useTieredSimilarImages` with a TS comment;
   migrate call sites over time. Both names work during the
   migration; eventually delete the old name.

The audit's identical-behaviour rule prefers (1) but flags (2) for
the implementing engineer.

**Justification.** N/A — both options are fine.

**Expected benefit.** Cosmetic only.

**Impact assessment.** None.

---

### I-FE-2 — `PLACEHOLDER_WIDTH` / `PLACEHOLDER_HEIGHT` are duplicated as private consts

- **Severity:** Low
- **Category:** Inconsistent Patterns
- **Location:** `src/services/images.ts:18-19`

**Current state.** `PLACEHOLDER_WIDTH = 400` and
`PLACEHOLDER_HEIGHT = 400` are private consts in `services/images.ts`,
used both by `fetchImages` (line 65-66) and `mapImageSearchResult`
(line 240). Any consumer outside this file that needs the same
fallback dimensions would have to re-derive them.

The Phase 12a comment explains the 1:1 square choice clearly. The
constants are correctly shared inside the file.

**Proposed change.** None. The duplication-risk is hypothetical
(no other consumer wants these values today). If a future consumer
appears, hoist to `src/types.d.ts` or a small
`src/constants.ts` then.

**Justification.** YAGNI.

**Expected benefit.** N/A.

**Impact assessment.** N/A.

---

### D-SET-1 — `Settings::priority_image_encoder` is documented as deprecated but `indexing.rs` still reads it

- **Severity:** Medium
- **Category:** Inconsistent Patterns / Documentation Rot
- **Location:**
  - `src-tauri/src/settings.rs:29-42` (deprecation docstring)
  - `src-tauri/src/indexing.rs:524-545` (active reader)

**Current state.** `Settings::priority_image_encoder` carries a
prominent deprecation docstring: "LEGACY (deprecated 2026-04-26 with
Phase 11c). [...] the value is ignored — the indexing pipeline reads
`enabled_encoders` instead. Will be removed once every install has
been bumped past pipeline-version 4."

But `indexing.rs::run_pipeline_inner:524-545` still reads the field
to determine which encoder's cosine cache to populate into the
primary `CosineIndexState`:

```rust
let priority = crate::settings::Settings::load()
    .priority_image_encoder
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "clip_vit_b_32".to_string());
```

So the docstring is wrong: the field is *not* ignored. The primary
`CosineIndexState` still serves the legacy single-encoder commands
(D-SIM-1 in `area-2-fusion-and-search.md`) — which are themselves
unreachable from production. Both layers need to be cleaned up
together.

**Proposed change.** Once D-SIM-1 + D-SEM-1 + D-COS-1 are acted on
(legacy single-encoder commands deleted), the safety-net populate at
`indexing.rs:524-545` becomes unreachable too — `CosineIndexState`
itself can be deleted. At that point the `priority_image_encoder`
field is genuinely unused and the deprecation docstring becomes true.

In the meantime: update the docstring at `settings.rs:29-42` to
reflect the *current* truth — "Field is read by `indexing.rs`'s
post-encoder cosine cache populate, which serves the legacy
single-encoder similarity commands. Will become unused once those
commands are removed (D-SIM-1)."

**Justification.** Comment-only correction. Identical behaviour.

**Expected benefit.** The deprecation docstring stops lying about
what the field does today.

**Impact assessment.** Comment-only edit.

---

### U-DEP-1 — `base64` dependency is unused

- **Severity:** Low
- **Category:** Unused Dependencies
- **Location:** `src-tauri/Cargo.toml:28`
- **Confidence:** High (`grep -r 'use base64\|base64::'` over
  `src-tauri/src` returns zero matches)

**Current state.** `base64 = "0.22.1"` is declared as a direct
dependency but no source file imports it. The TLS / model download
path (`model_download.rs`) does not use base64.

**Proposed change.** Remove the line from `Cargo.toml`.

**Justification.** Verified zero usages.

**Expected benefit.** Smaller dep graph, slightly faster builds, one
less crate to audit for security.

**Impact assessment.** None — `cargo build` will succeed without it.

---

### D-MAIN-1 — `let _ = profiling;` placeholder in `main.rs`

- **Severity:** Low
- **Category:** Dead Code
- **Location:** `src-tauri/src/main.rs:47`

**Current state.** Line 47 reads `let _ = profiling; // currently unused; kept above for future use`.
The comment claims the binding is kept "for future use," but the
`profiling` variable is read on lines 56 and 27 already. The
discard binding does nothing.

**Proposed change.** Delete line 47.

**Justification.** Identical behaviour; the variable is used on
adjacent lines.

**Expected benefit.** One fewer dead line.

**Impact assessment.** None.

---

### D-MAIN-2 — `--profile` vs `--profiling` naming drift in comments

- **Severity:** Low
- **Category:** Documentation Rot
- **Location:**
  - `src-tauri/src/main.rs:6-29` says `--profile` *AND* `--profiling`
    in different places
  - `src-tauri/src/perf.rs:88-107` says `--profile`
  - `src-tauri/src/lib.rs:296` says `--profile`

**Current state.** The actual CLI flag is `--profiling` (per
`main.rs:27`, `std::env::args().any(|a| a == "--profiling")`). Several
comments still reference `--profile` — this includes the docstring
on `is_profiling_enabled()` ("True if this process was launched with
`--profile`") and a comment on the `lib.rs` setup callback ("startup
diagnostic [...] if perf::is_profiling_enabled()" with a `--profile`
reference upstream).

**Proposed change.** Replace `--profile` with `--profiling` in every
comment that refers to the CLI flag. Keep `--profile` in any
historical reference (e.g. "the perf-1777212369 baseline was
captured with --profile").

**Justification.** Comment-only correction.

**Expected benefit.** Future readers don't try to invoke the wrong
flag.

**Impact assessment.** Comment-only.

---

### I-PERF-1 — Phase 7 sampler doesn't gate `record_diagnostic` (already gated internally)

- **Severity:** Low (info-only — no actual issue)
- **Category:** Inconsistent Patterns

**Current state.** `spawn_system_sampler_thread` (perf.rs:317) checks
`is_profiling_enabled()` at the top and bails if false. Inside the
loop it calls `record_diagnostic("system_sample", ...)` which itself
checks the same flag. Two checks; the first one is sufficient.

**Proposed change.** None. The internal check inside `record_diagnostic`
is the safer default — every other call site relies on it. Removing
the check inside `record_diagnostic` would couple correctness to
"every caller already gated on `is_profiling_enabled()`," which is
brittle. Leave as-is.

**Justification.** N/A.

---

### I-PERF-2 — `sysinfo::ProcessRefreshKind::new()` may be deprecated in newer sysinfo

- **Severity:** Low
- **Category:** Documentation Rot (forward-looking)
- **Location:** `src-tauri/src/perf.rs:335-336`

**Current state.** Uses `sysinfo::ProcessRefreshKind::new().with_cpu().with_memory()`.
sysinfo 0.32 is pinned (`Cargo.toml:60`); newer versions (0.33+)
deprecated `new()` in favour of `nothing()`. Not a current issue —
the pinned version still has `new()`. Will need updating on next
sysinfo bump.

**Proposed change.** None now. Note for future dependency bump:
sysinfo's API changed `ProcessRefreshKind::new()` → `nothing()`
between 0.32 and 0.33.

**Justification.** N/A.

**Expected benefit.** Future bump session has a heads-up.

---

### M-PERF-1 — `perf_report.rs` (892 lines) is verbose but cohesive

- **Severity:** Low (acknowledgement, not a recommendation)
- **Category:** Modularisation

**Current state.** 892 lines, sectioned into one function per report
section (header, top by total, hotspots, outliers, stall analysis,
resource trends, action timeline, per-span table, diagnostics, footer).
Each section function is ~30-100 lines, plus diagnostic-rendering
helpers.

**Proposed change.** None. The size reflects the report having many
sections; splitting into per-section files would scatter helpers
shared between sections (`format_us_human`, `format_payload`,
`event_ts`, `event_is_*`).

**Justification.** Single concern (markdown rendering). `leave-as-is`.

---

## R-tag annotations sweep

`grep -c "// R[0-9]" src-tauri/src/**/*.rs` returns 30 annotations
across 8 files. Spot-checked — every annotation refers to an
actually-implemented R<n> from `plans/perf-optimisation-plan.md`. No
dead R-tag references found. The R-tag pattern is documented in
`notes/conventions.md` § "R-tag perf annotation pattern (introduced
2026-04-26)" and being followed.

No findings.

## TODO comment sweep

`grep -rn "TODO\|FIXME\|XXX" src-tauri/src --include='*.rs'` returns
zero hits. Clean.

## Clippy gate

`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
runs clean. The `clippy::doc_lazy_continuation` allow at `lib.rs:9`
is documented and intentional.

No clippy-related findings.

## Modularisation verdict for `lib.rs`

`leave-as-is` (522 lines). The file is the Tauri Builder composition
+ State types + setup callback. Splitting would scatter the
`manage(...)` calls and the setup-time wiring (legacy migration,
indexing spawn, watcher start) across multiple files. The current
single-file shape lets a reader see the whole app composition in one
scroll.
