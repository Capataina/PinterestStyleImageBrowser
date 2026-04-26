---
source_type: github-rfc
date_published: 2026-04
hype_score: 0
---

# Tauri 2 — Dialog Plugin (Folder Picker)

## Source reference

- Tauri Dialog plugin docs: https://v2.tauri.app/plugin/dialog/
- Rust API: https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/struct.FileDialogBuilder.html

## Claim summary

Official Tauri 2 plugin. Provides `pick_folder` and `pick_folders` (multi-folder) dialogs on desktop platforms. Selected paths automatically added to filesystem and asset-protocol scopes. **Android folder-picker not yet implemented.**

## Relevance to our project

A3: The project just shipped multi-folder support (commit `0908550`) and folder picking (commit `47435f9` "Pass 4a: native folder picker"). Confirms the right tooling choice — the official Tauri plugin.

The "selected paths added to asset-protocol scopes" is the dynamic-scope mechanism that lets the project escape its `scope: ["**"]` over-permissive default.

## Specific takeaways

- The plugin is the canonical solution for the project's needs.
- The dynamic-scope behaviour eliminates the security loose end documented in `Suggestions.md` R3.
- Android limitation is irrelevant for the project (desktop only).

## Hype indicators

None — official docs.
