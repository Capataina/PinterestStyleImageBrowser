# Cross-Cutting — Code Health Findings

**Systems covered:** Rust static health, encoder allocation hygiene, dependency manifest
**Finding count:** 3 findings (1 medium, 2 low)

## Inconsistent Patterns

### Restore Strict Clippy As A Usable Health Gate
- [ ] Resolve the 33 strict-Clippy errors and add `cargo clippy --all-targets --all-features -- -D warnings` to the regular verification path.

**Category:** Inconsistent Patterns
**Severity:** medium
**Effort:** small
**Behavioural Impact:** none to negligible; float literal precision changes should be reviewed but are representationally equivalent at `f32` precision.

**Location:**
- `src/db/roots.rs:145` — unused import
- `src/commands/semantic.rs:146` — redundant closure
- `src/commands/similarity.rs:135` — redundant closure
- `src/db/images_query.rs:75` — complex aggregate tuple
- `src/db/schema_migrations.rs:106` — `matches!` pattern
- `src/filesystem.rs:17` — `new_without_default`
- `src/paths.rs:49` — manual prefix strip
- `src/perf.rs:334` — `io::Error::other`
- `src/perf_report.rs:100` — redundant closure
- `src/similarity_and_semantic_search/encoder_text/encoder.rs:243` — unnecessary allocation

**Current State:**
`cargo test` passes, but `cargo clippy --all-targets --all-features -- -D warnings` fails with 33 errors. Most are mechanical, but the failure means strict static analysis cannot be used as a regression gate. A few errors also point at real maintainability issues, particularly the anonymous aggregate tuple in `db/images_query.rs` and the unnecessary `to_vec()` in the text encoder.

**Proposed Change:**
Fix the Clippy output in one bounded cleanup pass. Keep the pass behaviour-free:

- Remove unused imports and redundant closures.
- Replace `io::Error::new(io::ErrorKind::Other, ...)` with `io::Error::other(...)`.
- Add `Default` where `new()` is a zero-argument constructor.
- Replace manual prefix stripping with `strip_prefix`.
- Introduce a private aggregate row struct/type alias for `images_query.rs`.
- Replace `normalize(&data[start..end].to_vec())` with `normalize(&data[start..end])`.

**Justification:**
The value is not aesthetic lint compliance; it is restoring a tool that catches low-level mistakes before review. Clippy's specific findings are mostly behaviour-preserving transformations, and the ones that are not purely mechanical are small enough to review directly.

**Expected Benefit:**
Turns strict Clippy from a permanently failing command into a usable CI/local check. It also removes small allocations and tuple-shape opacity that would otherwise keep reappearing in reviews.

**Impact Assessment:**
Most edits are semantics-preserving by construction. The only review-sensitive item is CLIP std literal truncation; because the values are stored as `f32`, Clippy's suggested grouped/truncated literal should represent the same practical value, but keep that change in the same test run as encoder diagnostics.

## Performance Improvement

### Remove Per-Batch Text-Embedding Slice Allocation
- [ ] Pass text-embedding slices directly to `normalize` instead of cloning each 512-float segment.

**Category:** Performance Improvement
**Severity:** low
**Effort:** trivial
**Behavioural Impact:** none.

**Location:**
- `src-tauri/src/similarity_and_semantic_search/encoder_text/encoder.rs:243` — `normalize(&data[start..end].to_vec())`
- `src-tauri/src/similarity_and_semantic_search/encoder_text/pooling.rs:36` — `normalize(vec: &[f32])`

**Current State:**
The text encoder extracts `data[start..end]`, clones that 512-float segment into a temporary vector, borrows the temporary as a slice, and then `normalize` immediately allocates the returned normalised vector. The temporary vector is unnecessary because `normalize` already accepts `&[f32]`.

**Proposed Change:**
Change the call to `normalize(&data[start..end])`.

**Justification:**
This removes one allocation and copy per text embedding in the batch path without touching the numerical operation. The Rust type signature is decisive evidence: `normalize` takes a slice, not ownership.

**Expected Benefit:**
Small but free reduction in semantic-search allocation churn, and one fewer Clippy failure.

**Impact Assessment:**
No behaviour change. `normalize` reads the same contiguous 512 floats and returns the same owned normalised vector.

## Unused Dependencies

### Remove Unused Direct Frontend Dev Dependencies
- [ ] Remove direct dev dependencies that have no imports or scripts in this repository.

**Category:** Unused Dependencies
**Severity:** low
**Effort:** trivial
**Behavioural Impact:** none if removed from direct dependencies only and the lockfile remains valid.

**Location:**
- `package.json:42` — `@testing-library/user-event`
- `package.json:48` — `baseline-browser-mapping`
- `package-lock.json` — corresponding root dependency entries

**Current State:**
`rg` finds no project imports or scripts for `@testing-library/user-event` or direct `baseline-browser-mapping`. `npm ls` shows `baseline-browser-mapping` is also brought transitively through `@vitejs/plugin-react` → `@babel/core` → `browserslist`, so the root direct dependency is redundant.

**Proposed Change:**
Remove the two direct dev dependencies and regenerate `package-lock.json` with `npm install` or the project's preferred lockfile update command. Keep `@vitest/coverage-v8`; it is used by `npm run test:coverage`.

**Justification:**
Direct dependency lists should describe what the project itself imports or invokes. Redundant root dependencies make upgrades noisier and imply ownership the project does not actually have.

**Expected Benefit:**
Slightly smaller manifest surface and clearer dependency ownership.

**Impact Assessment:**
No runtime behaviour change. The failure mode is hidden test code being added before this cleanup lands; rerun `rg '@testing-library/user-event|userEvent|baseline-browser-mapping'` immediately before removing.
