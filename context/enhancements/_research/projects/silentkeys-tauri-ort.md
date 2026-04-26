---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# SilentKeys — Tauri + ORT On-Device Dictation

## Source reference

- GitHub: https://github.com/gptguy/silentkeys

## Claim summary

Real-time, privacy-first, low-latency push-to-talk dictation. Stack: **Tauri v2 + Leptos UI + Rust core + ONNX Runtime (ORT) + Parakeet ASR + Silero-VAD**. Audio never leaves device. Zero telemetry. Live development on Apple Silicon, builds available for Linux/Windows.

## Relevance to our project

A1 + A2 + A3: **The closest analogue project** — same architectural triple (Tauri + Rust + ORT) with the same on-device-ML + privacy stance. The user's project is image-search, SilentKeys is speech-to-text — different ML domain, identical engineering pattern.

## Specific takeaways

- This validates the entire architectural choice. Multiple production apps in the Tauri+Rust+ORT category exist.
- SilentKeys uses Leptos (Rust frontend); the project uses React 19. Both are valid Tauri-frontend choices.
- The "on-device only, never sends audio" framing maps 1:1 to the project's "on-device only, never sends images" framing.
- Audience-fit reinforcement: this kind of project is becoming a recognisable category.

## Hype indicators

Mild — early-stage GitHub project with personal-blog tone, but the code is real and the architecture is verifiable.
