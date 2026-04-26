---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# Rust EXIF Metadata — kamadak-exif and rexiv2

## Source reference

- kamadak-exif: https://github.com/kamadak/exif-rs
- rexiv2: https://github.com/felixc/rexiv2

## Claim summary

Two Rust EXIF libraries. **kamadak-exif** is pure Rust, parses (read-only) EXIF from JPEG/TIFF/HEIF/PNG/WebP. **rexiv2** wraps the C `exiv2` library; supports read+write of EXIF/XMP/IPTC.

## Relevance to our project

A3 + A1: The project's `Roadmap.md` line 85 lists "EXIF awareness — date, location, camera — massively richer filtering without any ML work" as a high-value direction. kamadak-exif is the right choice (pure Rust, matches the project's "no C deps in the hot path" stance). One additional `Cargo.toml` entry + a small extraction module + DB schema migration adds date/location/camera to the filter system.

## Specific takeaways

- kamadak-exif: 600+ stars, pure Rust, actively maintained.
- EXIF date parsing is the "captured at" metadata users expect for photo browsing — turning the project from "all images shuffled" to "browse by year/month".
- GPS EXIF data enables a future map-view filter; relevant for personal-photography use cases.

## Hype indicators

None — utility library.
