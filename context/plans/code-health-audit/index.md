# Code Health Audit

**Date:** 2026-04-26
**Scope:** full repository (`src-tauri/`, `src/`, scripts, context plans/docs)
**Status:** active

## Summary

The audit found 7 actionable findings: 1 high, 4 medium, and 2 low. The highest-priority item is a real cache-coherence bug in the cosine search state: root changes can clear the cache without clearing the encoder marker, causing the next similarity query to return zero results despite embeddings existing in SQLite. The rest of the findings are behaviour-preserving health work: split the indexing pipeline and frontend route at existing seams, restore strict Clippy as a usable gate, remove a small text-encoder allocation, trim unused direct dev dependencies, and refresh stale context docs.

The codebase is in materially better shape than a typical audit target: baseline tests and build pass, the partial top-k cosine optimisation is already implemented and tested, and the large profiling modules are cohesive enough to leave alone for now.

## Findings Overview

| File | System | Critical | High | Medium | Low | Total |
|---|---|---:|---:|---:|---:|---:|
| [backend-state-and-indexing.md](backend-state-and-indexing.md) | Backend state / indexing | 0 | 1 | 1 | 0 | 2 |
| [frontend-and-docs.md](frontend-and-docs.md) | Frontend / docs | 0 | 0 | 2 | 0 | 2 |
| [cross-cutting.md](cross-cutting.md) | Project-wide hygiene | 0 | 0 | 1 | 2 | 3 |
| **Total** |  | **0** | **1** | **4** | **2** | **7** |

## Priority Actions

1. **[HIGH]** Fix cosine cache invalidation so root changes reset both cache contents and encoder marker — [backend-state-and-indexing.md#cosine-cache-can-stay-marked-current-after-being-cleared](backend-state-and-indexing.md#cosine-cache-can-stay-marked-current-after-being-cleared)
2. **[MEDIUM]** Restore strict Clippy as a usable health gate — [cross-cutting.md#restore-strict-clippy-as-a-usable-health-gate](cross-cutting.md#restore-strict-clippy-as-a-usable-health-gate)
3. **[MEDIUM]** Split indexing pipeline phases behind the existing public API — [backend-state-and-indexing.md#split-indexing-pipeline-phases-behind-the-existing-public-api](backend-state-and-indexing.md#split-indexing-pipeline-phases-behind-the-existing-public-api)
4. **[MEDIUM]** Extract route state from the 516-line Home component — [frontend-and-docs.md#extract-route-state-from-the-516-line-home-component](frontend-and-docs.md#extract-route-state-from-the-516-line-home-component)
5. **[MEDIUM]** Refresh context files that still describe pre-push state — [frontend-and-docs.md#refresh-context-files-that-still-describe-pre-push-state](frontend-and-docs.md#refresh-context-files-that-still-describe-pre-push-state)

## By Category

- Known Issues and Active Risks: 1 finding
- Modularisation: 2 findings
- Documentation Rot: 1 finding
- Inconsistent Patterns: 1 finding
- Performance Improvement: 1 finding
- Unused Dependencies: 1 finding

## What I Did Not Do

For each obligation, status is `done`, `partial`, or `skipped`.

- **Pre-Pass-1 front-loaded WebSearch:** done — query `code health audit patterns for Rust Tauri React desktop app`; recorded in `obligation-evidence-map.md`.
- **Pass-1 checkpoint file written:** done — `context/plans/code-health-audit/PASS-1-CHECKPOINT.md`.
- **Pass-2 systems-audited file written:** done — `context/plans/code-health-audit/PASS-2-SYSTEMS-AUDITED.md`.
- **Obligation Evidence Map populated for every substantive system:** done — `context/plans/code-health-audit/obligation-evidence-map.md`.
- **WebSearch call per substantive system:** done — every substantive row has research evidence or a reasoned omission in the evidence map.
- **Research mode variety across queries:** done — modes 1, 2, and 3 are all represented.
- **Diagnostic tests written where they would resolve moderate-to-high uncertainty:** done — `src-tauri/tests/cosine_cache_invalidation_diagnostic.rs` was added for the cache-coherence bug. Other findings were static or extraction-only and did not need new diagnostics.
- **Data-layout and memory-access analysis applied to every audited system:** done — evidence map records applicability; the text-encoder allocation and `images_query` aggregate row shape are the emitted findings.
- **Every modularisation candidate received a verdict:** done — see `PASS-2-SYSTEMS-AUDITED.md`.
- **Project test/build baseline captured:** done — `npm test`, `cargo test`, `npm run build`, and `cargo clippy --all-targets --all-features -- -D warnings` results are recorded across the checkpoint and verification notes.
- **Production source edits:** skipped by design — the skill forbids production source edits. Only an ignored diagnostic test and audit plan files were added.

## Verification Notes

Baseline evidence captured during the audit:

| Command | Result | Notes |
|---|---|---|
| `npm test --silent` | pass | 4 Vitest files, 53 tests. |
| `cargo test` | pass with warning | 107 lib tests + 6 cosine diagnostic + 6 indexing pipeline tests passed; 2 real-image integration tests ignored. Warning: unused import in `src/db/roots.rs`. |
| `npm run build` | pass | `tsc && vite build` succeeded. |
| `cargo clippy --all-targets --all-features -- -D warnings` | fail | 33 Clippy errors; captured as a finding rather than silently fixed. |
| `cargo test --test cosine_cache_invalidation_diagnostic` | pass | Diagnostic test is ignored by default. |
| `cargo test --test cosine_cache_invalidation_diagnostic -- --ignored` | expected fail | Demonstrates the current cache-invalidation bug until the production fix lands. |
| `python3 /Users/atacanercetinkaya/.codex/skills/code-health-audit/scripts/evidence_map_lint.py context/plans/code-health-audit/obligation-evidence-map.md` | pass | 18 rows inspected; research modes `[1, 2, 3]` detected. |

One attempted verification command, `npm test -- --runInBand`, was invalid because `--runInBand` is a Jest flag, not a Vitest flag. It was not used as evidence.

## Assumption And Failure Mode

**Assumption needing stronger evidence:** the root-change cache bug is user-visible in normal interaction after remove/toggle operations. The diagnostic proves the state-machine failure, but I did not run the GUI to reproduce the end-to-end UI symptom.

**Counter-scenario / failure mode:** if a pipeline run happens to repopulate the priority encoder immediately after a root change, the user may never observe the empty-cache state. The risk still bites when `remove_root` or `set_root_enabled` clears the cache without spawning a repopulating pipeline, or when a query lands in the gap before a spawned pipeline finishes.
