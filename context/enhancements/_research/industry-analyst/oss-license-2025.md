---
source_type: industry-analyst
date_published: 2025-12
hype_score: 1
---

# Open Source License Landscape 2025 (OSI)

## Source reference

- OSI: https://opensource.org/blog/top-open-source-licenses-in-2025
- Choose-a-License: https://choosealicense.com/

## Claim summary

OSI 2025 data: 80% of OSS projects use MIT, Apache 2.0, GPL-3.0, or BSD-2-Clause. **GitHub's 2025 survey: 70% of repositories use permissive licenses (MIT, Apache, BSD).** AGPL is the relevant copyleft option for "I want this open even when run as a service".

## Relevance to our project

A1 + A2 + A3: Image Browser is currently unlicensed (per repo state). A licensing recommendation for the project: **Apache 2.0 or MIT for permissive-with-patent-protection** — matches Tauri (Apache 2.0), `ort` (Apache 2.0), most of the project's deps. AGPL would *not* fit a desktop app where the project is run on the user's machine.

## Specific takeaways

- Apache 2.0 is the most defensible choice — matches the project's main dependencies.
- MIT is the simplest choice if patent protection is not needed.
- AGPL would harm portfolio signal here (overly restrictive for the use case).
- License choice is portfolio-relevant — visible in the GitHub repo header.

## Hype indicators

Mild — OSI content is authoritative; Dev.to overviews are SEO-shaped but consistent.
