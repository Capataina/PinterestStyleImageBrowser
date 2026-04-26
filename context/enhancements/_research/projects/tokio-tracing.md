---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# tokio-rs/tracing — Rust Application-Level Tracing

## Source reference

- Crates: https://docs.rs/tracing
- GitHub: https://github.com/tokio-rs/tracing
- Luca Palmieri's "Are we observable yet?": https://www.lpalmieri.com/posts/2020-09-27-zero-to-production-4-are-we-observable-yet/

## Claim summary

`tracing` is the modern Rust observability backbone. Spans + events with structured fields, parent/child causality, async-correlated. `#[instrument]` proc-macro auto-spans functions. Subscriber model decouples emission from consumption (file, OTLP, JSON, custom).

## Relevance to our project

A1 + A3: The project already migrated to `tracing` in commit `7918e39` ("Pass 6: tracing migration") and built a custom Subscriber Layer for the perf-diagnostics overlay. The recommendation downstream is to layer in `tracing-subscriber` standard formatters + `tracing-opentelemetry` exporter for an "in-process app + OTLP-export" production posture.

## Specific takeaways

- The project's `PerfLayer` is a custom `tracing-subscriber::Layer`. This is the canonical extension pattern.
- OTLP exporter via `tracing-opentelemetry` adds a full-fat observability story: Jaeger / Tempo / Honeycomb / SigNoz can consume the same spans.
- For a desktop-app perf-diagnostics use case, the JSONL + on-exit markdown report (already shipped) is the right shape; OTLP is the optional next layer.

## Hype indicators

None — foundational ecosystem crate.
