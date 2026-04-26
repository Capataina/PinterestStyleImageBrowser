# Pass 2 systems-audited checkpoint — 2026-04-26

Static snapshot of the Pass-2 deep dive. Distinct from
`obligation-evidence-map.md` (which is appended live during the audit
and may carry per-row evidence updates).

| # | System | Research evidence | Diagnostic test | Findings | Confidence |
|---|--------|-------------------|-----------------|---------:|-----------|
| 1 | `indexing.rs` | WebSearch ort 2.0 multi-thread anti-patterns + SQLite WAL multi-writer | `audit_indexing_parallel_encoder_diagnostic.rs` | 6 (D-IDX-1, D-IDX-2, D-IDX-3, I-IDX-1, I-IDX-2, I-IDX-3, K-IDX-1, M-IDX-1) | High |
| 2 | `commands/{semantic,semantic_fused,similarity}.rs` + `cosine/rrf.rs` | Cormack 2009 RRF (in-repo references) + WebSearch ort/SQLite | `audit_fusion_no_text_capable_encoders_diagnostic.rs` | 6 (D-SIM-1, D-SEM-1, D-FUS-1, K-FUS-1, M-FUS-1, D-COS-1) | High for D-SIM-1/D-SEM-1 (frontend grep proof); Moderate for D-COS-1 (depends on pipeline-version 3 wipe being installed) |
| 3 | Encoder modules + `ort_session.rs` + `preprocess.rs` | WebSearch ort 2.0 thread-local memory leaks (mode 3) | (covered by indexing diagnostic) | 5 (D-ENC-1, D-ENC-2, I-ENC-1, D-ENC-3, K-ORT-1, I-ENC-4) | High for D-ENC-1, D-ENC-2; Moderate for K-ORT-1 |
| 4 | `db/mod.rs` + `db/embeddings.rs` | WebSearch SQLite WAL multi-writer + Oldmoe blog | `audit_db_read_lock_routing_diagnostic.rs` | 5 (I-DB-1 (= I-ENC-4), D-DB-1, I-DB-2, I-DB-3, K-DB-1, M-DB-1) | High |
| 5 | Frontend dispatch + `perf.rs` + `perf_report.rs` + `settings.rs` + general sweep + Cargo.toml | (no per-system WebSearch needed — frontend changes are locally verifiable; perf has explicit phase doc; deps verified by grep) | none — frontend / Cargo edits are mechanical | 9 (D-FE-1, D-FE-2, I-FE-1, I-FE-2, D-SET-1, U-DEP-1, D-MAIN-1, D-MAIN-2, I-PERF-1, I-PERF-2, M-PERF-1) | High |

Total findings: **30** (counting each lettered finding once; I-DB-1 is
listed under both area-3 and area-4 with the same identifier).

## Per-system Data Layout / Memory Access analysis

The audit obligation requires an explicit per-system applicability
decision for Data Layout and Memory Access Patterns. Recorded here:

| System | Applicable? | Notes |
|--------|-------------|-------|
| `indexing.rs` | No new findings | The encoder phase already uses per-thread DB connections (avoids mutex churn) and `BEGIN IMMEDIATE` batched writes (R1). The buffer-flattening opportunities are in `encoder.rs` (CHW separation), already implemented; no slack remains at this layer. |
| Fusion + search | No new findings | RRF uses HashMap aggregation — O(N) on the union of top-K's, ~150 entries × 3 encoders = ~450 entries per call. HashMap is appropriate. CosineIndex already uses `select_nth_unstable_by` partial sort (per `cosine/index.rs:14-19`) and reusable scratch buffers, eliminating inner-loop allocations. |
| Encoders | No new findings | Each encoder's `preprocess` allocates a `Vec<f32>` of size `3 × CROP × CROP` and three temporary planes. Avoidable via SIMD planar conversion, but the allocation cost is dominated by ONNX inference itself. Not worth a finding at current scale. |
| DB | No new findings | Embeddings stored as raw f32 byte BLOBs via `bytemuck::cast_slice` — already zero-copy at the f32-to-byte boundary. Foreground SELECTs via the read-only secondary already avoid mutex contention. |
| Frontend / perf / misc | Not applicable | Frontend and TS code are not data-layout-sensitive at this scale. Perf module's RawEvent is a small enum — Vec-based ringbuffer is documented to be fine at the cap chosen. |

## Pass-2 acceptance

All ten systems in the Pass-1 prioritisation now have:

- A research entry (or a reasoned omission) in
  `obligation-evidence-map.md`.
- Findings recorded in the per-area files.
- Modularisation verdict (in `obligation-evidence-map.md`).
- Data Layout decision (above).

Audit moves to final output.
