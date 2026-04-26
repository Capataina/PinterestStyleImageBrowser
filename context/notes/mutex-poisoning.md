# mutex-poisoning

## Current Understanding

Five long-lived sync primitives live for the lifetime of the app:

| Primitive | Owner | Acquired by |
|-----------|-------|-------------|
| `Mutex<rusqlite::Connection>` | each `ImageDatabase` instance (foreground + background indexing) | every DB method (~30 sites across `db/`) |
| `Arc<Mutex<CosineIndex>>` | `CosineIndexState.index` (shared with indexing thread) | every similarity / semantic command + `cosine.populate_from_db` + `cosine.save_to_disk` |
| `Mutex<Option<TextEncoder>>` | `TextEncoderState.encoder` | `commands::semantic::semantic_search` + indexing pipeline pre-warm |
| `Arc<Mutex<Option<WatcherHandle>>>` | `watcher_state` (slot for the debouncer handle) | lib.rs setup callback |
| `Arc<IndexingState>` (`AtomicBool`) | `indexing_state` | every command that triggers an index + watcher debounce closure |

DB methods use `.lock().unwrap()` — the project treats Mutex poisoning as unrecoverable; a panic with the lock held should bring down the session and force a restart. Tauri command bodies use `?` (which routes through the `From<PoisonError<T>> for ApiError` impl) so poisoning surfaces as `ApiError::Cosine("mutex poisoned: ...")` to the frontend instead of crashing the Tauri process.

If any code panics while holding one of the Mutexes, the lock is poisoned for the rest of the session — every subsequent `.lock()` on it returns `Err(PoisonError)`. Recovery requires restarting the app. The user gets typed errors instead of vague stringly-typed ones thanks to the `From<PoisonError>` impl, which is an improvement over the pre-typed-error state.

## Why the contention pressure is now real

WAL means foreground reads no longer block background writes (`systems/database.md`). But the cosine `Arc<Mutex<CosineIndex>>` is shared across the indexing thread (writes via `populate_from_db` + `save_to_disk`) AND the foreground commands (reads via the 3 retrieval methods). Concurrent foreground similarity queries during the cosine_repopulate phase contend on this Mutex; the foreground waits a few hundred ms for the populate to finish.

Pre-Phase-5 the indexing pipeline ran inside `main()` (blocking pre-Tauri). The cosine Mutex contention didn't matter because the foreground couldn't issue queries while indexing was happening. Now it can — and does, briefly.

This makes the cost of a poison panic higher than it was. A panic during `populate_from_db` poisons the Arc<Mutex>, and every subsequent foreground similarity query fails until restart.

## Rationale

The choice of `Mutex` over `RwLock` or `parking_lot::Mutex` was implicit: standard library defaults. The poisoning behaviour is std-Mutex's safety mechanism — a partially-mutated state is exposed as an explicit error rather than silently presented as valid.

For a single-user desktop app, the practical implication is: if any panic happens, the affected subsystem becomes unusable until restarted. The user sees typed-but-vague errors after the first failure (`ApiError::Cosine("mutex poisoned: ...")`).

## Guiding Principles

- **The current safety mechanism is restart.** Tauri restarts are fast. The pragmatic posture is: poison-then-restart is acceptable; flailing in a partially-broken state is not.
- **Do not silently `lock().unwrap_or_else(|p| p.into_inner())`** — recovering from poison without understanding what state survived is worse than restarting.
- **`From<PoisonError<T>> for ApiError` over `unwrap()` in command bodies** — the typed signal lets the frontend show a real error message instead of the user wondering why nothing works.
- **`parking_lot::Mutex` is a strict upgrade** if poisoning becomes a real annoyance. It does not poison and is faster. The downgrade is one less safety check; for this codebase, that is acceptable. Documented in `enhancements/recommendations/09-typed-error-enum-and-mutex-replacement.md`.
- **`catch_unwind` at command boundaries** would convert backend panics into typed errors without poisoning the mutex. Not currently implemented — the typed-error From-impl path is the lighter intervention.

## What Was Tried

Nothing in version control switched away from std-Mutex. The poisoning behaviour has not bitten in production because the project's test corpus does not produce panics during normal operation. The risk is theoretical until something like a malformed image, a corrupted DB, or a change in ort versions surfaces a panic path.

The typed-error migration (commit `cda7caa`) made the poison case observable: the user now sees `ApiError::Cosine("mutex poisoned: ...")` instead of the previous opaque "Search failed" string. This is an improvement but doesn't solve the underlying recovery problem.

## Trigger to revisit

- A real session loses functionality after a single panic and the user reports it.
- A new Tauri command path holds two mutexes simultaneously (currently the most-locks-held-at-once is `semantic_search`, which holds the text encoder Mutex *then* the cosine Mutex — non-overlapping in scope).
- Cross-session poisoning becomes observable in any non-trivial QA run.
- The cosine Mutex contention measured in profiling exceeds a comfortable threshold (today's brief contention during the populate phase is acceptable).

At that point: `parking_lot::Mutex` swap, then add `catch_unwind` at command bodies as a defence-in-depth.

## Naming inconsistency

The `From<PoisonError<T>> for ApiError` impl always maps to `ApiError::Cosine` regardless of which mutex was actually poisoned. The source comment in `commands/error.rs` acknowledges this is imprecise — a poisoned `TextEncoderState.encoder` shows as "cosine error: mutex poisoned". Functionally fine (the recovery is the same: restart) but worth fixing if the diagnostic precision matters.
