# mutex-poisoning

## Current Understanding

Three `Mutex`-protected singletons live for the lifetime of the app:

| Mutex | Owner | Acquired by |
|-------|-------|-------------|
| `Mutex<rusqlite::Connection>` | `ImageDatabase` | every DB method (20+ sites) |
| `Mutex<CosineIndex>` | `CosineIndexState` | every similarity / semantic command |
| `Mutex<Option<TextEncoder>>` | `TextEncoderState` | only `semantic_search` |

Each `.lock()` call uses `.unwrap()` (DB methods) or `.map_err(...)` to a stringified error (Tauri command bodies). If any code panics while holding one of these locks, the lock is poisoned for the rest of the session — every subsequent `lock()` returns `Err(PoisonError)` and is unrecoverable without restarting the app.

## Rationale

The choice of `Mutex` over `RwLock` or `parking_lot::Mutex` was implicit: standard library defaults. The poisoning behaviour is std-Mutex's safety mechanism — a partially-mutated state is exposed as an explicit error rather than silently presented as valid.

For a single-user desktop app, the practical implication is: if any panic happens (unwrap on a malformed DB row, an out-of-bounds index in cosine math, an ONNX session-run failure that escalates), the app becomes unusable until restarted. The user sees vague errors after the first failure.

## Guiding Principles

- **The current safety mechanism is restart.** Tauri restarts are fast. The pragmatic posture is: poison-then-restart is acceptable; flailing in a partially-broken state is not.
- **Do not silently `lock().unwrap_or_else(|p| p.into_inner())`** — recovering from poison without understanding what state survived is worse than restarting.
- **Add `catch_unwind` only at command boundaries**, not deep in the stack. The intent is to convert a backend panic into an `Err(String)` that crosses the IPC, surfacing a real error to the frontend rather than crashing the Tauri process. Today's behaviour escalates panics into unrecoverable Mutex poisoning, which then surfaces as `Mutex poisoned` strings on every subsequent call — confusing and unhelpful.
- **`parking_lot::Mutex` is a strict upgrade** if poisoning becomes a real annoyance. It does not poison and is faster. The downgrade is one less safety check; for this codebase, that is acceptable.

## What Was Tried

Nothing in version control switched away from std-Mutex. The poisoning behaviour has not bitten in production because the project's test corpus does not produce panics during normal operation. The risk is theoretical until something like a malformed image, a corrupted DB, or a change in ort versions surfaces a panic path.

## Trigger to revisit

- A real session loses functionality after a single panic and the user reports it.
- A new Tauri command path holds two mutexes simultaneously (currently the most-locks-held-at-once is `semantic_search`, which holds the text encoder Mutex *then* the cosine Mutex — non-overlapping).
- Cross-session poisoning becomes observable in any non-trivial QA run.

At that point: `parking_lot::Mutex` swap, then add `catch_unwind` at command bodies.
