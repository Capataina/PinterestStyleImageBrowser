---
audience: Tauri + modern-React desktop-app engineers
secondary_audiences: Local-first / privacy-engineering community
coupling_grade: plug-and-play
implementation_cost: small (2-3 days)
status: draft
---

# Tauri 2 hardening — restrictive CSP + dynamic asset-protocol scope

## What the addition is

Two coupled hardenings to the Tauri shell:

1. **Replace `csp: null` with a restrictive Content-Security-Policy** in `tauri.conf.json`:
   ```json
   "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline'; script-src 'self'; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost"
   ```
2. **Replace `assetProtocol.scope: ["**"]` with dynamic scope** that mirrors the user's currently-configured library roots. When the user adds a folder via the existing folder picker (commit `47435f9`), call `scope.allow(path)`; on remove, `scope.forbid(path)`. Use the Tauri 2 ACL APIs introduced for v2.

A short `context/notes/security-posture.md` documents the threat model: WebView is sandboxed from system, asset protocol now bounded to declared roots, CSP prevents injected-script RCE.

## Audience targeted

**Primary: A3 Tauri + modern-React desktop-app engineers** — `audience.md` Audience 3 signal-function: "IPC discipline: Typed Tauri command surface, narrow `assetProtocol.scope`, named error enum surviving the boundary" and "What doesn't [score]: `csp: null`, `scope: ["**"]`, stringified errors". Today the project hits both anti-patterns — fixing them is high-leverage signal.

**Secondary: A2** — "Local-by-construction" + "Threat-model clarity" both gain from this rec. Combined with Rec-7 (encrypted vectors), the project can claim a clean "what does this app expose" story.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/rfcs-and-issues/tauri-asset-protocol-csp.md` | Official Tauri 2 docs explicitly recommend restrictive CSP + scoped asset protocol. The current `csp: null` + `scope: ["**"]` is the documented anti-pattern. |
| 2 | `_research/rfcs-and-issues/tauri-dialog-folder-picker.md` | The Tauri 2 dialog plugin's `pick_folder` automatically extends scopes — the runtime mechanism for dynamic scope is already in place. |
| 3 | `_research/projects/tauri2-stable.md` | Tauri 2 went stable 2024-10; the ACL-based command/scope mechanism is mature. |
| 4 | `_research/projects/awesome-tauri.md` | Production Tauri apps (SilentKeys, Spacedrive, Hoppscotch) all set restrictive CSP + scoped asset protocol — industry norm. |
| 5 | `_research/projects/spacedrive.md` | Spacedrive is an explicit reference: same architectural family, scoped security. |
| 6 | `_research/projects/silentkeys-tauri-ort.md` | SilentKeys' "audio never leaves device + zero telemetry" stance is built on the same Tauri 2 security primitives. |
| 7 | `_research/forums/tauri-vs-electron-2025.md` | Tauri's security narrative vs Electron is a marketing differentiator; the project should *actually have* the security posture, not just claim it. |
| 8 | `_research/notes` (vault) — `Suggestions.md` R3 | The user's own vault explicitly flags this as a security loose end. |
| 9 | `_research/notes` (project) — `local-first-philosophy.md` | The project's own design principles document privacy as construction-time; this rec actualises the principle. |
| 10 | `_research/firm-hiring/anthropic-infra-rust.md` | Anthropic specifically hires Sandboxing Engineers — the role family this rec demonstrates competence for. |

## Coupling-grade classification

**Plug-and-play.** Configuration change + small runtime hook. No data structure changes. If the new CSP breaks something, revert by changing two lines back. If the dynamic scope causes issues, fall back to the old `["**"]` (which is what every Tauri tutorial defaults to anyway).

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, with permissive CSP and asset-protocol scope (intentional during development).** This rec narrows the Tauri shell's exposure to match the project's stated local-first / privacy-by-construction stance — the security posture catches up to the README claim.

```
   Today                                After Rec-8
   ──────────────────────────           ──────────────────────────────
   tauri.conf.json:                     tauri.conf.json:
     "csp": null                          "csp": "default-src 'self'; …"
     scope: ["**"]                        scope: []  (filled at runtime
                                                     from user folder
                                                     selection)

   No runtime scope updates             folder add → scope.allow(path)
                                        folder remove → scope.forbid(path)
                                                  │
                                                  ▼
                                        WebView can only load images from
                                        explicitly-selected library roots.
```

The existing folder picker (commit `47435f9`) and multi-folder support (commit `0908550`) provide the natural hook points. The new code is small: one allow-list update on folder-add, one forbid on folder-remove.

## Anti-thesis

This recommendation would NOT improve the project if:

- The project ships only to the user himself and is never shared. Then "permissive scope is fine" is defensible. But even for personal use, the act of hardening is the audience-readable signal.
- The CSP breaks something subtle (e.g., a shadcn primitive depending on inline scripts). That's why the CSP allows `'unsafe-inline'` for styles — most shadcn/ui works under that constraint, but it needs verification.
- Tauri 2 ACL APIs change in a future minor version. They are stable per the v2.0 release; risk is low.

## Implementation cost

**Small: 2-3 days.**

Milestones:
1. Set the restrictive CSP in `tauri.conf.json`. Run the app; identify any breakages (missing `script-src`, missing `connect-src` for IPC, inline-style assumption from a primitive). Fix incrementally. ~1 day.
2. Replace static scope with empty initial scope; add the `scope.allow(path)` / `scope.forbid(path)` hooks in the multi-folder Tauri commands. ~1 day.
3. Test the full flow: folder add → scope updated → image loads via asset protocol → folder remove → scope reduced → image no longer loads. ~½ day.
4. Document the threat model in `context/notes/security-posture.md`. ~½ day.

Required reading before starting: Tauri 2's CSP guide (`https://v2.tauri.app/security/csp/`) and the dialog-plugin docs for the scope-update API.
