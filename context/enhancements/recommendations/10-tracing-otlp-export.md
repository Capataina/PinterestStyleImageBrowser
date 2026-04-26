---
audience: Rust + applied-ML systems engineers
secondary_audiences: Tauri + modern-React desktop-app engineers
coupling_grade: plug-and-play
implementation_cost: small (2-3 days)
status: draft
---

# OpenTelemetry OTLP exporter for the existing tracing perf-diagnostics system

## What the addition is

A new optional `tracing-opentelemetry` + `opentelemetry-otlp` dependency, gated behind a config flag (`[telemetry] otlp_endpoint = "..."`). When enabled, the project's existing `tracing` spans (commit `7918e39` migrated everything to tracing) are also exported via OTLP to the configured endpoint — Jaeger, Tempo, Honeycomb, SigNoz, or any standard OTLP collector. The existing `PerfLayer` (commit `2f32f74` and the JSONL + report flush) stays unchanged — OTLP is *additional* output, not a replacement.

A small `docs/observability.md` describes the spans the project emits + the recommended OTLP collector setup for users who want to plug it into their own observability stack.

## Audience targeted

**Primary: A1 Rust + applied-ML systems engineers** — `audience.md` Audience 1 signal-function: "API surface: `Result<T, ApiError>` enums, `tracing` spans, `#[instrument]` boundaries". The project already has the spans; OTLP export turns them into a *production-grade* observability story, the most credible signal a desktop app can send to an SRE-adjacent reviewer.

**Secondary: A3 Tauri + modern-React** — desktop-app perf instrumentation that integrates with industry-standard observability tooling is rare; visible Tauri-app credibility win.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/projects/tokio-tracing.md` | tracing-opentelemetry + opentelemetry-otlp is the canonical Rust observability stack. |
| 2 | `_research/projects/firefox-translate-onnx.md` | Firefox ships ONNX-Runtime locally — equivalent observability discipline applies; production reference. |
| 3 | `_research/projects/silentkeys-tauri-ort.md` | Reference Tauri+ORT app has tracing-shaped observability; the OTLP export is the next layer. |
| 4 | `_research/projects/spacedrive.md` | Production Rust+Tauri apps emit telemetry through tracing pipelines. |
| 5 | `_research/firm-hiring/anthropic-infra-rust.md` | Anthropic's Infrastructure roles specifically value observability (it's an "Infrastructure / Sandboxing" specialty). |
| 6 | `_research/firm-hiring/cloudflare-workers-ai.md` | Cloudflare's Workers AI infrastructure runs deeply tracing-instrumented production code; the artefact maps. |
| 7 | `_research/papers/onnx-int8-quantization.md` | Quantisation introduces measurable accuracy drift — observability is the way to monitor for regressions in production. |
| 8 | `_research/projects/criterion-rs.md` | Cross-coupling: criterion microbenchmarks (Rec-2/Rec-6) and OTLP traces tell different stories — micro vs in-vivo. Both are valuable. |
| 9 | `_research/notes` (project) — `perf-diagnostics.md` master plan | The project's own perf-diagnostics plan explicitly anticipates this evolution. |
| 10 | `_research/notes` (project) — `conventions.md` | Documents that tracing should replace println. The OTLP export is the natural next step. |

## Coupling-grade classification

**Plug-and-play.** Two new deps, one config flag, one new tracing-subscriber Layer registered alongside the existing `PerfLayer`. The JSONL + on-exit report path stays. Removing the rec deletes the deps + config. The PerfLayer pipeline is unaffected.

## Integration plan

**The project today is a local-first Tauri 2 desktop app with a custom `PerfLayer` writing tracing spans to JSONL + producing an on-exit markdown report (commits `2f32f74` / `26c16e8` / `765ce33`).** This rec adds a second Subscriber Layer that also exports the same spans via OTLP when configured.

```
   Today                           After Rec-10
   ──────────────────────          ──────────────────────────
   tracing::Span                    tracing::Span
       │                                │
       ▼                                ▼
   tracing-subscriber               tracing-subscriber
       │                                │
       ├─ PerfLayer (existing)          ├─ PerfLayer (existing)
       │   ├ JSONL                      │   ├ JSONL
       │   └ on-exit report.md          │   └ on-exit report.md
       │                                │
                                        └─ OpenTelemetryLayer (new)
                                              │
                                              ▼
                                          OTLP endpoint
                                          (Jaeger / Tempo /
                                           Honeycomb / SigNoz)
```

Default config: OTLP disabled. Power users add `[telemetry] otlp_endpoint = "http://localhost:4317"` to `Library/config.toml` (the same config file used in Rec-2 / Rec-3) to enable export. The desktop app's existing perf overlay continues to render JSONL — the OTLP path is purely additive.

## Anti-thesis

This recommendation would NOT improve the project if:

- The user runs only on a single machine without an observability backend. The existing JSONL + markdown-report path is sufficient. Then the OTLP exporter is dead config; small cost.
- The OTLP endpoint adds cold-start latency in shipped builds. Mitigation: only initialise the exporter when the config flag is set.
- Future tracing-opentelemetry / opentelemetry-otlp API churn breaks the build. The OTel ecosystem has had churn historically; an abstraction layer mitigates risk.

## Implementation cost

**Small: 2-3 days.**

Milestones:
1. Add `tracing-opentelemetry`, `opentelemetry-otlp`, `opentelemetry_sdk` to Cargo.toml. ~½ day (build-system fiddling included).
2. Wire the optional layer registration in `main.rs` after the existing PerfLayer. Gated by config. ~½ day.
3. Add `Library/config.toml` parsing for `[telemetry]` section. (May need to design the config-file schema if not already in place; the user has `Library/config.toml` for some QoL prefs per commit `a66d1f7`.) ~½ day.
4. Test against a local Jaeger or Tempo container. ~½ day.
5. Document in `docs/observability.md`: which spans the app emits, recommended collector setup. ~½ day.

Required reading before starting: `context/plans/perf-diagnostics.md` for the project's existing tracing model.
