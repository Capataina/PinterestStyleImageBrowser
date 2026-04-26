---
source_type: forum-blog
date_published: 2025-12
hype_score: 2
---

# Framer Motion / Motion (v12) — React Animation Performance Best Practices

## Source reference

- Motion.dev: https://motion.dev
- Motion v12 Release: https://motion.dev/docs/react-motion-component
- "Animations That Don't Kill Performance": https://dev.to/whoffagents/framer-motion-animations-that-dont-kill-performance-patterns-and-pitfalls-5cki

## Claim summary

Framer Motion was renamed to "Motion" mid-2025 (`framer-motion` → `motion/react`). v12 brought hardware-accelerated scroll, layoutAnchor, axis-locked layout. Hardware-accelerated transform/opacity → 120fps; animating layout-affecting properties (width/height/bg-color) breaks performance. **For 50+ animated items, virtualize via react-window / react-virtuoso.**

## Relevance to our project

A3: The project uses framer-motion 12.23 (per `Architecture.md` deps table) for the masonry tilt, modal, and entrance animations. With hundreds of images, this becomes a perf concern at scale. The recommendation downstream: virtualize the masonry grid (react-virtuoso or custom intersection-observer), animating only on-screen items.

The project just shipped a perf-diagnostics overlay (commits `2f32f74` / `765ce33`); the animation cost is exactly the kind of metric the overlay should highlight.

## Specific takeaways

- "Animate transform + opacity, never layout-affecting properties" is the canonical guideline.
- Virtualisation is the right scaling answer; the masonry packer is *already* a custom layout engine, so adding viewport-aware mounting is incremental work.
- The tilt-on-hover cost is per-item; on-screen-only mounting bounds it.

## Hype indicators

Mild — DEV.to post is SEO-shaped; underlying motion.dev docs are authoritative.
