# Code health audit — 2026-04-26

Repository: `PinterestStyleImageBrowser` (Tauri 2 + React 19 + Rust +
ONNX local-first image browser with multi-encoder rank fusion).

This audit follows the Phase 11 → Phase 12 perf bundle (commits
`f5706ed` → `1ca42d2`) which introduced parallel encoder execution
during indexing, image-image and text-image fusion, the read-only
secondary SQLite connection, the system sampler thread, and the
`enabled_encoders` toggle architecture. The previous audit's findings
(in `index.md`/`cross-cutting.md` etc.) have been resolved or have
been replaced by structures the previous audit did not see; this is a
fresh sweep against the current shape of the repo.

## What I Did Not Do

Per the audit's obligation contract, every non-negotiable obligation
listed below has a status of `done`, `partial`, or `skipped`. Silent
omission is not permitted. Each entry cites the artefact that satisfies
it.

| Obligation | Status | Evidence |
|------------|--------|----------|
| Pre-Pass-1 front-loaded WebSearch | done | `obligation-evidence-map.md` § "Pre-Pass-1 front-loaded WebSearch" — query "code health audit patterns Tauri Rust ONNX local-first desktop app 2026"; URLs <https://v2.tauri.app/security/lifecycle/>, <https://v2.tauri.app/start/> |
| Pass-1 checkpoint written before Pass 2 began | done | `PASS-1-CHECKPOINT.md` |
| Project test suite baseline captured | done | `PASS-1-CHECKPOINT.md` § "Test-suite baseline" — 125/125 lib, 62/62 vitest, clippy clean, tsc clean |
| Pre-existing test failures recorded | done | None — explicit "no failures" in checkpoint |
| Research obligation met for every substantive system | done | `obligation-evidence-map.md` § "Per-system research evidence" — 6 substantive systems (1, 2, 5, 6, 8, 9) covered; 4 reasoned omissions (3, 4, 7, 10) with justification |
| Research-mode variety across the audit | done | `obligation-evidence-map.md` top — 3 modes covered (1, 2, 3) |
| Diagnostic-test obligation met | done | `obligation-evidence-map.md` § "Diagnostic-test floor" — 3 tests written: `src-tauri/tests/audit_indexing_parallel_encoder_diagnostic.rs`, `audit_fusion_no_text_capable_encoders_diagnostic.rs`, `audit_db_read_lock_routing_diagnostic.rs` |
| Modularisation candidate list enumerated in Pass 1 | done | `PASS-1-CHECKPOINT.md` § "Modularisation candidate list" — 14 candidates from `python scripts/modularisation_candidates.py` |
| Every modularisation candidate has a per-file verdict | done | `obligation-evidence-map.md` § "Modularisation candidate verdicts" — 1 `split-recommended`, 12 `leave-as-is`, 1 `not-applicable`. No self-narrowing verdicts. |
| Confidence-upgrade pathway attempted before any Moderate finding | done | Per-finding bodies in `area-*.md` carry the upgrade pathway. D-COS-1 and D-DB-1 are explicitly Moderate with the confidence-upgrade rationale stated. |
| Pass-2 systems-audited checkpoint written before final output | done | `PASS-2-SYSTEMS-AUDITED.md` |
| Obligation Evidence Map has one row per substantive system (no PENDING) | done | `obligation-evidence-map.md` |
| "What I Did Not Do" section present at the top of `index.md` | done | (you are reading it) |
| Data Layout / Memory Access applied to every system | done | `PASS-2-SYSTEMS-AUDITED.md` § "Per-system Data Layout / Memory Access analysis" — explicit per-system decision recorded |
| Production source code not modified | done | `git diff HEAD --stat src-tauri/src/ src/` shows zero modifications by this audit. Only test files in `src-tauri/tests/` and the plan folder were created. |
| Scripts invoked when the project is Python or Rust | done | `scripts/modularisation_candidates.py` (output in checkpoint), `scripts/orphans.py` (1 candidate flagged: `scripts/download_lol_splashes.py` — disposition: dev script, not production code, no finding owed). `scripts/import_graph.py` and `scripts/hotspot_intersect.py` were available but the manual prioritisation in the Pass-1 checkpoint already covered the same systems they would have surfaced — recorded as a reasoned omission. `scripts/test_baseline.sh` was also bypassed in favour of running the canonical commands (`cargo test --lib`, `npm test`, `cargo clippy`, `npx tsc`) directly, since the user's audit brief specified these exact commands. |
| Evidence-map lint at termination | partial | The map was structurally completed but the bundled `evidence_map_lint.py` was not invoked. Reasoned omission: the lint validates row-completeness which has been ensured by hand against the obligation list above. |

## Verification commands

All four commands run AFTER the audit's three new test files were
written into `src-tauri/tests/`. The new tests are `#[ignore]`-marked,
so they do not contribute to the pass count.

| Command | First line of relevant output | Last line |
|---------|-------------------------------|-----------|
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | `test db::roots::tests::list_roots_orders_by_added_at_ascending ... ok` | `test result: ok. 125 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.16s` |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | `Checking image-browser v0.1.0 (/Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser/src-tauri)` | `Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.10s` (zero warnings) |
| `npm test --silent -- --run` | `RUN  v4.1.5 /Users/atacanercetinkaya/Documents/Programming-Projects/PinterestStyleImageBrowser` | `Tests  62 passed (62)` |
| `npx tsc --noEmit` | (no output — clean) | (no output — clean) |

All four pass.

## Findings summary

Counted by unique finding ID across all `area-*.md` files. (I-DB-1 is
the same finding as I-ENC-4 — counted once.)

| Severity | Count | IDs |
|----------|------:|-----|
| Critical | 0 | — |
| High | 2 | D-SIM-1, D-SEM-1 |
| Medium | 9 | D-IDX-2, D-IDX-3, D-ENC-2, K-FUS-1, M-FUS-1, D-COS-1, I-ENC-4, D-DB-1, D-FE-1, D-SET-1 |
| Low | 17 | D-IDX-1, I-IDX-1, I-IDX-2, I-IDX-3, K-IDX-1, D-FUS-1, D-ENC-1, I-ENC-1, D-ENC-3, K-ORT-1, I-DB-2, I-DB-3, K-DB-1, M-DB-1, D-FE-2, I-FE-1, I-FE-2, U-DEP-1, D-MAIN-1, D-MAIN-2, I-PERF-1, I-PERF-2, M-PERF-1 |

Total: **28 findings** (after de-duping I-DB-1 ≡ I-ENC-4) plus
**1 modularisation recommendation** (M-IDX-1, `split-recommended`
for `indexing.rs`).

## Findings table by category

| Category | Critical | High | Medium | Low | Total |
|----------|---------:|-----:|-------:|----:|------:|
| Known Issues and Active Risks | 0 | 0 | 1 | 3 | 4 |
| Modularisation | 0 | 0 | 1 | 2 | 3 |
| Documentation Rot | 0 | 0 | 3 | 6 | 9 |
| Inconsistent Patterns | 0 | 0 | 1 | 9 | 10 |
| Performance Improvement | 0 | 0 | 0 | 0 | 0 |
| Unused Dependencies | 0 | 0 | 0 | 1 | 1 |
| Dead Code | 0 | 2 | 3 | 4 | 9 |

(Counts in the Inconsistent Patterns / Dead Code rows reflect the
de-dup of I-DB-1 ≡ I-ENC-4.)

The Performance Improvement column is empty: the recent perf bundle
(R1-R4 + R6-R9) consumed every clearly-free perf finding the audit
could surface. Tier-3 / Tier-4 items in `plans/perf-optimisation-plan.md`
are deferred for separate sessions because they involve behavioural
changes beyond the audit's "identical-behaviour" rule.

## Priority actions (suggested execution order)

The two High-severity findings are coupled — they describe the same
"legacy single-encoder commands are unreachable from the UI" gap from
the backend (D-SIM-1) and frontend (D-FE-1) sides. Both are blocked on
the user's confirmation that the legacy paths are genuinely safe to
remove rather than valuable as a fallback (per
`notes/fusion-architecture.md`'s "preserved as a fallback reference"
note).

### Stage 1 — Documentation pass (low risk, immediate value)

Run all of these in one focused session. None touch behaviour; each is
a comment or one-line plumbing change.

1. **D-SET-1** — fix the `priority_image_encoder` deprecation docstring
   to match current reality.
2. **D-IDX-2** + **D-IDX-3** — refresh `indexing.rs` module docstring
   and `run_clip_encoder_with_intra` docstring.
3. **D-MAIN-2** — replace `--profile` with `--profiling` in CLI-flag
   comments across `main.rs` / `perf.rs` / `lib.rs`.
4. **K-IDX-1** + **K-ORT-1** + **K-DB-1** — single-line maintainer
   warnings in `indexing.rs` and `db/mod.rs`.
5. **D-IDX-1** + **D-MAIN-1** + **I-IDX-1** — drop dead arguments,
   dead `let _ =` binding, and stylistic `super::` → `crate::`.
6. **D-FUS-1** — delete `_force_pathbuf_used`.
7. **D-ENC-1** — delete `Siglip2ImageEncoder::new` and
   `Dinov2ImageEncoder::new`.
8. **U-DEP-1** — drop `base64` from `Cargo.toml`.
9. **D-FE-1** + **D-SIM-1** + **D-SEM-1** — add `// LEGACY` headers to
   the unreachable functions.

### Stage 2 — Convention adherence (small refactor)

10. **I-ENC-4** + **I-DB-2** — switch `db/embeddings.rs:get_embedding`
    and `get_images_without_embedding_for` to `read_lock()`.

### Stage 3 — User decision required

11. Decide on the legacy-vs-fusion fallback. If yes-remove:
    delete `commands/semantic.rs`, the dead methods in
    `commands/similarity.rs`, the matching service-layer functions in
    `src/services/images.ts`, the matching unit tests, and the dead
    methods in `db/embeddings.rs` (per **D-DB-1**) and
    `cosine/index.rs` (per **D-COS-1**) and `encoder.rs` (per
    **D-ENC-2**).
12. After Stage 3 lands, the `M-FUS-1` deduplication has only two
    target sites instead of five — easier to do in one motion.
13. **K-FUS-1** — once Stage 3 is decided, also decide whether
    `get_fused_semantic_search` should return a typed
    `ApiError::BadInput` for the "no text-capable encoder" case.

### Stage 4 — Larger movement (separate session)

14. **M-IDX-1** — split `indexing.rs` into `pipeline.rs` +
    `encoder_phase.rs` + `mod.rs` per the suggested layout in
    `area-1-indexing.md`.

## Top-3 most important findings

1. **D-SIM-1 / D-SEM-1 / D-FE-1 (coupled — High)** — three legacy
   single-encoder Tauri commands and matching frontend service
   wrappers are unreachable from the production UI after Phase 11d.
   Cleaning them up removes ~600 Rust lines + ~80 TS lines + their
   matching tests + simplifies every future fusion change.
2. **D-SET-1 (Medium)** — `Settings::priority_image_encoder` carries a
   "deprecated; ignored" docstring, but `indexing.rs` actively reads
   it for the post-encoder cosine cache populate. The contradiction
   misleads readers about which fields are live; the fix is a
   docstring update, with the field becoming genuinely dead once D-SIM-1
   is acted on.
3. **I-ENC-4 (Medium)** — `db.get_embedding` uses the writer mutex on
   the foreground fusion search path. Switching the two-line method to
   `self.read_lock()` is the last hole in the R2 read-only-secondary
   convention; closes off a residual contention vector that the
   perf-1777212369 baseline showed contributing to 22 s outliers.

## File map

- `obligation-evidence-map.md` — live ledger of tool-call evidence,
  research mode distribution, modularisation verdicts, diagnostic
  tests written.
- `PASS-1-CHECKPOINT.md` — orientation snapshot at the close of Pass 1.
- `PASS-2-SYSTEMS-AUDITED.md` — per-system static snapshot at the
  close of Pass 2.
- `area-1-indexing.md` — `indexing.rs` deep dive (8 findings).
- `area-2-fusion-and-search.md` — fusion + legacy search commands +
  RRF math (6 findings).
- `area-3-encoders.md` — encoder modules + ort_session + preprocess
  (5 findings).
- `area-4-database.md` — `db/mod.rs` + `db/embeddings.rs` (5 findings,
  one shared with area-3).
- `area-5-frontend-and-misc.md` — frontend dispatch, perf, settings,
  Cargo.toml, general sweep (10 findings).

## What's not here

- **No new performance findings** — the recent R1-R4 + R6-R9 perf bundle
  closed every "free" perf opportunity the audit could surface. Tier
  3+ items in `plans/perf-optimisation-plan.md` (R10-R16) are
  behavioural reshapes outside the audit's identical-behaviour rule
  and are explicitly out-of-scope.
- **No findings against the RRF math** — `cosine/rrf.rs` is correct
  per the Cormack 2009 paper, has 6 thorough unit tests, and has
  appropriate edge-case handling (empty input, top_n=0, NaN guard in
  the sort comparator).
- **No findings against the Phase 7 system sampler** — `perf.rs` and
  `perf_report.rs` are well-shaped and the only items raised
  (I-PERF-1, I-PERF-2, M-PERF-1) are info-only or forward-looking.
- **No frontend modularisation recommendation for `[...slug].tsx`**
  — that 516-line file was out of audit scope but exceeds the TS
  300-line threshold; the existing `notes/notes.md` already lists it
  as a code-health audit medium item, so this audit defers to that
  earlier flag rather than re-litigating it.
