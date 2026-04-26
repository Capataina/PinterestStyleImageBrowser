# Frontend And Documentation — Code Health Findings

**Systems covered:** frontend route state, repository context documents
**Finding count:** 2 findings (2 medium)

## Modularisation

### Extract Route State From The 516-Line Home Component
- [ ] Extract search/result selection, notes loading, shortcuts/profiling, and folder-add logic from `src/pages/[...slug].tsx` into route-local hooks.

**Category:** Modularisation
**Severity:** medium
**Effort:** medium
**Behavioural Impact:** none if limited to extraction.

**Location:**
- `src/pages/[...slug].tsx:28` — `Home`
- `src/pages/[...slug].tsx:110` — notes loading effect
- `src/pages/[...slug].tsx:157` — `displayImages` derivation
- `src/pages/[...slug].tsx:213` — URL selection effect
- `src/pages/[...slug].tsx:366` — folder-add flow

**Current State:**
The Home route component owns route state, semantic-search activation, similar-image activation, selected-item URL reconciliation, inspector notes loading/saving, settings/profiling shortcuts, root addition, first-launch empty state, search headers, and the masonry render. Several of those concerns already have clear boundaries in the code, but they still share one component body, so changes to one concern create avoidable risk in the others.

**Proposed Change:**
Extract route-local hooks and small presentational sections without changing query keys or UI behaviour:

- `useHomeSearchState` for `searchTags`, `searchText`, debounced semantic activation, and `displayImages`.
- `useSelectedImageRoute` for URL-to-selected-item reconciliation and previous/next navigation.
- `useImageNotesState` for lazy notes loading/saving.
- `useHomeShortcuts` for settings and profiling shortcuts.
- `AddFolderButton` or `useAddFolderFlow` for the folder picker duplicate check and mutation.

Keep the existing query hooks (`useImages`, `useSemanticSearch`, `useTieredSimilarImages`, `useRoots`) as the data boundary.

**Justification:**
TanStack Query recommends targeted invalidation/background refetching rather than hand-maintained normalised caches; this route is already using query hooks correctly. The issue is that the UI orchestration layer now mixes unrelated state transitions in one component. Extracting hooks preserves the current query model while making each transition easier to test and reason about.

**Expected Benefit:**
The route becomes a composition surface instead of the owner of every workflow. Future work on semantic search, similar-image navigation, or root management can be tested and reviewed independently.

**Impact Assessment:**
Extraction should not affect behaviour if dependencies and query keys are preserved. The failure mode is stale closure/dependency mistakes in the extracted hooks; run `npm test`, `npm run build`, and manual smoke tests for semantic search, similar-image navigation, notes save, and adding a duplicate folder.

## Documentation Rot

### Refresh Context Files That Still Describe Pre-Push State
- [ ] Update stale context files so the repository memory matches the current pushed code.

**Category:** Documentation Rot
**Severity:** medium
**Effort:** small
**Behavioural Impact:** none.

**Location:**
- `context/notes.md:7` — says `105/105` lib tests and `not yet committed`
- `context/notes.md:13` — lists pipeline parallelism as active despite implementation being present
- `context/notes.md:14` — lists pipeline stats UI as pending despite frontend/backend wiring being present
- `context/plans/pipeline-parallelism-and-stats-ui.md:161` — unchecked completion criteria now contradict the code
- `context/architecture.md:23` — says 23 commands
- `context/architecture.md:97` and `context/architecture.md:188` — diagrams still say `invoke_handler![22 commands]`
- `src-tauri/src/lib.rs:313` — actual invoke handler registers 25 commands

**Current State:**
The code has moved past several context files. The active plan still describes pipeline parallelism and stats UI as upcoming work, while `indexing.rs` already overlaps thumbnail and encoder work and `get_pipeline_stats` is registered in the Tauri command surface. `context/architecture.md` contains both older command counts and newer rows that mention 25 command-related systems, so readers get mixed signals.

**Proposed Change:**
After the audit work is accepted, run a targeted context cleanup or an `upkeep-context` pass:

- Mark the pipeline parallelism/stats UI plan as complete or rewrite it as a retrospective note.
- Update command counts to 25 throughout `context/architecture.md`.
- Replace the `not yet committed` line in `context/notes.md` with the current pushed-state summary and current test counts.

**Justification:**
This project uses `context/` as operational memory. Stale active plans are not harmless prose; they cause the next session to spend effort re-discovering whether already-shipped work still needs doing.

**Expected Benefit:**
Reduces startup/orientation errors and prevents duplicate planning around work already shipped.

**Impact Assessment:**
Documentation-only. The failure mode is over-updating aspirational README direction based on current code; limit this finding to implementation-facing `context/` drift unless the user explicitly wants README direction revised.
