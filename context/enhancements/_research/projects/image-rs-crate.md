---
source_type: shipped-project
date_published: 2026-04
hype_score: 0
---

# image-rs / image crate — Rust Image Codec + Resize

## Source reference

- GitHub: https://github.com/image-rs/image
- Docs: https://docs.rs/image/latest/image/
- Transloadit blog: https://transloadit.com/devtips/optimizing-image-processing-in-rust-with-parallelism-and-rayon/

## Claim summary

The canonical Rust image-codec library. Decodes/encodes JPEG, PNG, GIF, BMP, TIFF, WebP. Resize filters: Nearest, Triangle, CatmullRom, Gaussian, Lanczos3. Lanczos3 is the recommended high-quality option — "smooth without too much performance cost".

## Relevance to our project

A1: The project already uses `image` crate. The recommendation is to swap `FilterType::Nearest` → `FilterType::Lanczos3` in CLIP preprocessing (per `clip-preprocessing-decisions.md`). One-line change with measurable embedding-quality improvement.

## Specific takeaways

- Lanczos3 is the standard for ML preprocessing (matches the OpenAI CLIP reference implementation's bicubic-equivalent quality).
- Nearest is appropriate for thumbnail previews where speed matters more than quality.
- The current "Nearest" choice is the documented gap; this is one of the cheapest known wins.

## Hype indicators

None.
