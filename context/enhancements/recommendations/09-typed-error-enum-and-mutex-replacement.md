---
audience: Tauri + modern-React desktop-app engineers
secondary_audiences: Rust + applied-ML systems engineers
coupling_grade: plug-and-play
implementation_cost: small (2-4 days)
status: draft
---

# Typed `ApiError` enum across Tauri IPC + `parking_lot::Mutex` swap

## What the addition is

Two coupled ergonomics + correctness improvements:

1. **Define `ApiError` enum in `lib.rs`** (or a new `src-tauri/src/api_error.rs`) implementing `serde::Serialize` and `Debug`. Variants cover the actual failure modes: `ImageNotFound { id }`, `EncoderUnavailable { reason }`, `IndexLockPoisoned`, `DatabaseError(String)`, `IoError(String)`, etc. Tauri commands return `Result<T, ApiError>`. Frontend `services/*.ts` receives a discriminated union and surfaces concrete error messages instead of the current generic "Search failed" string.
2. **Replace `std::sync::Mutex` with `parking_lot::Mutex`** for the three long-lived singletons: DB connection, CosineIndex, TextEncoder. `parking_lot::Mutex` doesn't poison on panic — a panic in one command no longer kills the entire session.

## Audience targeted

**Primary: A3 Tauri + modern-React desktop-app engineers** — `audience.md` Audience 3 signal-function: "IPC discipline: Typed Tauri command surface, narrow `assetProtocol.scope`, named error enum surviving the boundary" — this rec ships the named error enum.

**Secondary: A1 Rust + applied-ML systems engineers** — replacing `std::sync::Mutex` with `parking_lot::Mutex` is a canonical Rust hardening. The user's vault `mutex-poisoning.md` and `Suggestions.md` R4 both flag this.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/projects/parking-lot-mutex.md` | parking_lot is 1.5× faster uncontended, 5× contended; no poisoning. The standard upgrade path. |
| 2 | `_research/notes` (project) — `mutex-poisoning.md` | The user has documented this as the known foot-gun; parking_lot is named as the strict upgrade. |
| 3 | `_research/notes` (vault) — `Suggestions.md` R4 | Vault Suggestions explicitly rec parking_lot for the three-singleton pattern. |
| 4 | `_research/notes` (project) — `conventions.md` | Documents the current `.lock().unwrap()` IPC error stringification as the convention to evolve. |
| 5 | `_research/projects/tokio-tracing.md` | Typed errors are the prerequisite for structured tracing — `tracing::error!(error = ?api_err, …)` is much richer than `error = "string"`. |
| 6 | `_research/projects/silentkeys-tauri-ort.md` | Reference Tauri-ORT app uses typed errors across the IPC boundary; production-shape convention. |
| 7 | `_research/projects/spacedrive.md` | Spacedrive's IPC surface uses typed result types; industry norm in mature Tauri apps. |
| 8 | `_research/firm-hiring/anthropic-infra-rust.md` | Anthropic's Sandboxing role specifically rewards "clean error handling". |
| 9 | `_research/projects/tanstack-query-optimistic.md` | The frontend's optimistic mutations would benefit from typed errors — concrete error types let the UI roll back differently per error variant (server-error vs validation-error). |
| 10 | `_research/forums/react-19-best-practices.md` | React 19's `useActionState` works best with typed errors, even for Tauri-only apps. |

## Coupling-grade classification

**Plug-and-play.** The `ApiError` enum is one new module; existing `Result<_, String>` returns are migrated one Tauri command at a time. The frontend can keep handling errors as strings during migration; once all commands are typed, the frontend can be upgraded to discriminated-union handling.

The Mutex swap is a one-line `Cargo.toml` add + a `use parking_lot::Mutex` in three files + simplification of the lock signatures (`.lock().unwrap()` becomes `.lock()`). Reverting is trivial.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, with stringified IPC errors and `std::sync::Mutex` singletons that poison on panic.** This rec replaces those two patterns with their idiomatic Rust upgrades.

```
   Before                              After
   ────────────────────────            ──────────────────────────
   #[command]                          #[command]
   fn semantic_search(...) ->          fn semantic_search(...) ->
       Result<Vec<...>, String> {          Result<Vec<...>, ApiError> {
     let db = state.lock()                let db = state.lock();   // no Result
       .map_err(|e| e.to_string())?;      // panic doesn't poison
     ...                                  ...
     Err(format!("Failed: {}", e))       Err(ApiError::EncoderUnavailable {
   }                                          reason: e.to_string()
                                          })
                                        }
```

Frontend `services/images.ts` `semantic_search` already does `try/catch`; it now also does `.tag === "EncoderUnavailable"` discrimination and surfaces a per-tag user message (e.g., "The text model failed to load. Open Settings → Models to download.").

```
   Three singletons get parking_lot:
   ┌──────────────────────────────────────┐
   │ State                                │
   │   db: parking_lot::Mutex<Connection> │
   │   cosine_state: parking_lot::Mutex<CosineIndex>│
   │   text_encoder_state: parking_lot::Mutex<Option<TextEncoder>>│
   └──────────────────────────────────────┘
       Panic in one command no longer poisons the others.
       The session survives.
```

## Anti-thesis

This recommendation would NOT improve the project if:

- The user prefers the simplicity of stringified errors (acceptable but signals less to A3). The Mutex swap is independently valuable and can be done without the ApiError change.
- A specific error case requires propagating a non-`Serialize`able value across the IPC boundary. Then keep that error as a `String` variant.
- The session-survives-panic property is undesirable (some users prefer "fail loudly" — but a desktop app should generally not crash on any individual error).

## Implementation cost

**Small: 2-4 days.**

Milestones:
1. Define `ApiError` enum + impls (`Display`, `Serialize`, `From<rusqlite::Error>`, `From<io::Error>`, etc.). ~½ day.
2. Migrate all 8+ Tauri commands to return `Result<T, ApiError>`. Update tests. ~1 day.
3. Add `parking_lot` to Cargo.toml; replace the three `std::sync::Mutex` with `parking_lot::Mutex`. Delete the `.unwrap()` on every `.lock()` call. ~½ day.
4. Upgrade the frontend `services/*.ts` files to handle the discriminated union. Improve error-message UX for each variant. ~1 day.
5. Update tests + add unit tests for error variants. ~½ day.
6. Update `context/notes/conventions.md` to reflect the new IPC error pattern (currently documents the stringified version as the convention). ~½ day.

Required reading before starting: `context/notes/conventions.md` (current pattern), `context/notes/mutex-poisoning.md` (known foot-gun), `Suggestions.md` R4 (the user's stated intent).
