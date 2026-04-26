---
source_type: shipped-project
date_published: 2025-12
hype_score: 3
---

# Spacedrive

## Source reference

- GitHub: https://github.com/spacedriveapp/spacedrive
- Site: https://spacedrive.app

## Claim summary

Open-source cross-platform file manager. **PRRTT stack**: Prisma + Rust + React + TypeScript + Tauri. Indexes local + cloud (S3, Drive, Dropbox, OneDrive, Azure, GCS) as first-class volumes. Iroh/QUIC for device-to-device. "Media View" layout for photo/video files.

## Relevance to our project

A3 (Tauri+React): The flagship Tauri 2 + Rust + React app. Stars: ~30k. The project is a credibility anchor for the entire stack — when audiences ask "is Tauri 2 a real choice for desktop apps?", Spacedrive is the answer.

A1: Validates the Rust-core + Tauri-shell + React-UI pattern at scale. The project lives inside the same architectural family.

## Specific takeaways

- Spacedrive's existence is the durability case for Tauri 2 as a desktop framework. The framework is not going away.
- Their Rust virtual-filesystem core is an analogue of the project's filesystem-scanner — different scope but same kind of work.
- They use Prisma for the DB; the project uses rusqlite directly. Different choice; both defensible.
- Active development (recent commits weekly), 30k stars, 100+ contributors.

## Hype indicators

Some marketing voice on the project's site, but the GitHub repo is substantive: real code, real tests, real shipped binaries.
