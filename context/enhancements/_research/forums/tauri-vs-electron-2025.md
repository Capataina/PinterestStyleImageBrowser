---
source_type: forum-blog
date_published: 2025-10
hype_score: 4
---

# Tauri vs Electron — performance / bundle / memory

## Source reference

- gethopp.app: https://www.gethopp.app/blog/tauri-vs-electron
- pkgpulse blog: https://www.pkgpulse.com/blog/electron-vs-tauri-2026
- tech-insider: https://tech-insider.org/tauri-vs-electron-2026/

## Claim summary

Multiple 2025-2026 benchmarks: Tauri bundles <10 MB vs Electron 100+ MB; Tauri idle 30-40 MB RAM vs Electron 200-300 MB. Hoppscotch's Electron→Tauri migration: 165 MB → 8 MB bundle. Tauri startup <0.5s vs Electron 1-2s.

## Relevance to our project

A3: Concrete numbers backing the project's stack choice (Decision D1). When the project ships a binary, "10 MB Tauri app" is a directly cite-able artefact for this audience.

## Specific takeaways

- Numbers are well-attested across multiple independent sources — not a single-vendor claim.
- The project's binary should ship at the lower end of the Tauri range (sub-15 MB) once stripped, given its scope.
- "70% memory reduction Electron→Tauri migration" is an industry meme worth being aware of.

## Hype indicators

Moderate. Multiple of these blogs are SEO-shaped ("96% smaller apps, 1 winner"). But the underlying numbers triangulate against substantive sources (Tauri's own docs, Hoppscotch's blog).
