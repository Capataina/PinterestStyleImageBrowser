---
source_type: shipped-project
date_published: 2024-10-02
hype_score: 1
---

# Tauri 2.0 Stable Release

## Source reference

- Tauri Blog: https://v2.tauri.app/blog/tauri-20/
- GitHub: https://github.com/tauri-apps/tauri
- Wikipedia: https://en.wikipedia.org/wiki/Tauri_(software_framework)

## Claim summary

Tauri 2.0 went stable on **October 2, 2024**, after two years of architectural refinement. Mobile support (iOS + Android), new ACL-based command access (per-window scopes), shrunken minimum bundle size to <600 KB.

## Relevance to our project

A3 (Tauri+React): The project is on Tauri 2 — the framework just hit stable. Stack durability is solid; ACL-based command access is a cleaner way to handle the project's `assetProtocol.scope: ["**"]` security loose-end (the project's vault `Suggestions.md` R3 explicitly flags this).

A1: Tauri 2 mobile support is the *next* dimension this project could span — same Rust core, same `ort` model, render on iOS / Android. Not recommended directly (the audience signal isn't strong) but worth knowing.

## Specific takeaways

- The ACL-based command access is the answer to the project's `csp: null` loose end. Migrating to per-window scopes is a directly applicable hardening recommendation.
- Bundle size <600 KB minimum is a credibility marker for the local-first audience.
- Stable since 2024-10 → durability case for the framework is now strong.

## Hype indicators

Mild — Tauri team's announcement post is necessarily promotional, but the framework is real and shipping production apps.
