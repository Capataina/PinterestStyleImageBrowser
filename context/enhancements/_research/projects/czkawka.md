---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# Czkawka / Krokiet — Rust Duplicate File / Image Finder

## Source reference

- GitHub: https://github.com/qarmin/czkawka
- v10.0 blog: https://medium.com/@qarmin/czkawka-krokiet-10-0-4991186b7ad1

## Claim summary

Mature Rust duplicate-finder app. Multi-threaded, scans 1.5M+ files. Krokiet (newer Slint-based UI) supersedes the GTK frontend due to cross-platform reliability issues with GTK. Uses perceptual hashing (pHash family) for similar-image detection.

## Relevance to our project

A3: A directly-comparable Rust desktop app. Validates the "Rust + media tools + multi-threading" pattern. The project's CLIP-based similarity is *more sophisticated* than Czkawka's pHash; combined with an explicit "find duplicates" feature, the project would be strictly superior on similarity quality.

A1: Strong example of multi-threaded Rust file processing — model for the project's parallel-thumbnail and parallel-encoding directions.

## Specific takeaways

- Czkawka uses Slint for cross-platform GUI — alternative to Tauri+React for similar problems.
- The project's "View Similar" + a planned "Find Duplicates" feature could pitch as "Czkawka-quality dedup with CLIP-embedding precision".

## Hype indicators

None.
