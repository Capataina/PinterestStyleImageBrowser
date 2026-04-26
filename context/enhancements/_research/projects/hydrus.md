---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# Hydrus Network

## Source reference

- GitHub: https://github.com/hydrusnetwork/hydrus
- Docs: https://hydrusnetwork.github.io/hydrus/

## Claim summary

Personal booru-style media tagger written in Python. Imports files and tags from disk + popular websites; content can be shared via user-run servers. Tag-first organisation rather than folder-first. Active development, 4k+ stars.

## Relevance to our project

A3 (Tauri+React): Hydrus is the most-shipped "tag-first local image browser" — proves the audience exists. But Hydrus is Python+Qt, not Tauri+Rust+React. The Image Browser project's stack choices are a clean modernisation of the same UX premise.

A1: Hydrus's tag-first model maps onto the project's existing tag system. The AND/OR semantics (already shipped per recent commit `56990b7`) align with what Hydrus users expect.

## Specific takeaways

- Hydrus has a "no cloud, all local" stance — same as Image Browser. There is a community of users who actively choose local-first photo tools.
- Hydrus's UI is Qt-1990s-aesthetic; Image Browser's React+masonry+framer-motion stack is a clean modernisation.
- The "rec 5: ML-assisted auto-tagging" recommendation downstream has a clear precedent — Hydrus has third-party AI taggers (`wd-e621-hydrus-tagger`).

## Hype indicators

None — long-running OSS project.
