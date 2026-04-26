---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# notify (notify-rs) — Cross-platform FS Watcher

## Source reference

- GitHub: https://github.com/notify-rs/notify
- Crates: https://crates.io/crates/notify

## Claim summary

Canonical Rust filesystem-watcher library. Uses inotify (Linux), FSEvents (macOS), ReadDirectoryChangesW (Windows). Used by alacritty, cargo-watch, deno, rust-analyzer, watchexec, zed.

## Relevance to our project

A1: The project already uses `notify` (per commit `0908550` "Multi-folder support + filesystem watcher + orphan detection"). Confirms the right library choice — same library every other major Rust desktop tool relies on.

A3: A live filesystem watcher in a desktop app is the difference between "snapshot at start" and "always up-to-date". The project ships this.

## Specific takeaways

- Production-grade, cross-platform.
- The "debounced rescan trigger frequency" mentioned in `context/plans/perf-diagnostics.md` is exactly what this library helps with.
- An obvious additive recommendation: instrument the watcher with `tracing` spans (already partially done) and surface the rescan latency in the perf overlay.

## Hype indicators

None — utility crate.
