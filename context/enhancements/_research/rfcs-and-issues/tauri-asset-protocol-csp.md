---
source_type: github-rfc
date_published: 2025-04
hype_score: 0
---

# Tauri 2 — Asset Protocol Scope + CSP Hardening

## Source reference

- Tauri Discussion #11498: https://github.com/orgs/tauri-apps/discussions/11498
- Tauri CSP docs (v2): https://v2.tauri.app/security/csp/
- Tauri config schema: https://schema.tauri.app/config/2

## Claim summary

Tauri 2's recommended security posture: enable CSP via `tauri.conf.json` with restrictive sources, narrow `assetProtocol.scope` to specific glob patterns rather than `["**"]`. CSP is parsed at compile time and nonce-injected into the WebView. ACL-based per-window command access is the v2 mechanism.

## Relevance to our project

A3 + A2: The project explicitly opens `csp: null` and `scope: ["**"]` (per `Suggestions.md` R3) — fine for personal use, but a flag for any audience reviewing the security posture. Hardening to scoped CSP + scoped asset protocol is a directly applicable additive recommendation that signals security awareness to A3.

## Specific takeaways

- The asset protocol scope should ideally be the user's selected library roots, dynamically updated as roots are added/removed (the project just shipped multi-folder + watcher in commit `0908550`, so the dynamic-scope path is natural).
- CSP should at minimum be `default-src 'self'; img-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline'; ...` rather than null.
- Tauri 2's ACL model lets specific commands be granted to specific windows — useful when adding admin-vs-user surfaces.

## Hype indicators

None — official Tauri docs.
