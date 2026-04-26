---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# parking_lot — Faster, Non-Poisoning Rust Mutexes

## Source reference

- GitHub: https://github.com/Amanieu/parking_lot
- Docs: https://docs.rs/parking_lot/latest/parking_lot/type.Mutex.html

## Claim summary

`parking_lot::Mutex` is **1.5× faster uncontended, up to 5× faster contended** than `std::sync::Mutex`. Compact: 1 byte vs std's box. Eventual fairness guarantee. **Crucially: no poisoning** — the mutex unlocks normally on panic, the next acquirer just gets the lock.

## Relevance to our project

A1: The project's `context/notes/mutex-poisoning.md` explicitly names parking_lot as a strict upgrade for the three project Mutex singletons (DB connection, CosineIndex, TextEncoder). The vault Suggestions.md R4 also flags this.

The recommendation downstream is direct: replace `std::sync::Mutex` with `parking_lot::Mutex` in the three singleton sites, removing the "panic kills the session" failure mode. This is a perfect plug-and-play swap.

## Specific takeaways

- One-line `Cargo.toml` add + `use parking_lot::Mutex` + lock-call signature simplification (`.lock()` no longer returns `Result`).
- Cleaner code: no more `.lock().unwrap()` everywhere; just `.lock()`.
- Removes the foot-gun the project's vault has documented across multiple notes.

## Hype indicators

None — utility crate, well-established.
