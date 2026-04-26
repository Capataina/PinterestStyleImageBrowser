---
audience: ML-infra and retrieval / embedding-systems researchers
secondary_audiences: Tauri + modern-React desktop-app engineers
coupling_grade: plug-and-play
implementation_cost: medium (1-2 weeks)
status: draft
---

# Auto-tagging via CLIP zero-shot + Find Duplicates via cosine threshold

## What the addition is

Two coupled features that derive from the project's existing CLIP infrastructure with minimal new code:

1. **`auto_tag_images` Tauri command** — for each image (or each image lacking auto-tags), encode a fixed dictionary of label prompts ("a photo of a {label}") via the existing text encoder, compute cosine vs the image's embedding, assign tags whose score exceeds a per-tag threshold. Default dictionary is project-relevant (~150-300 labels: portrait, landscape, animal, urban, macro, food, vehicle, indoor, outdoor, daytime, night, abstract, etc.) — user-configurable via a config file or in-app editor.
2. **`find_duplicates` Tauri command** — for each image, query the existing `VectorIndex` (Rec-1) for top-K nearest neighbours; flag pairs with cosine ≥ 0.99 as near-duplicates. Surface in a "Find Duplicates" panel with side-by-side comparison + delete affordance.

Both features cleanly use what's already there (text encoder, image encoder, cosine index) — no new ML, no new infrastructure.

## Audience targeted

**Primary: A4 ML-infra and retrieval / embedding-systems researchers** — `audience.md` Audience 4 signal-function: "Retrieval mechanism" + "Quality audit". Auto-tagging is the canonical "what can you do with a unified embedding space beyond the obvious"; the four-derivative-features list (similar-search, semantic-search, auto-tag, dedup) is the Pinterest "unified visual embeddings" pattern.

**Secondary: A3** — closes the feature-parity gap with PhotoPrism / Hydrus / Eagle / Immich. Visible UX features that users see immediately.

## Why it works

| # | Source | Sub-claim |
|---|--------|-----------|
| 1 | `_research/papers/clip-zero-shot-classification.md` | CLIP zero-shot via cosine vs label prompts is the canonical technique. Trivially implementable; the project already has every primitive. |
| 2 | `_research/papers/perceptual-hash-vs-cnn-dedup.md` | CNN-embedding-based dedup outperforms perceptual-hash dedup. The project's CLIP infra is *already* superior to Czkawka's pHash. |
| 3 | `_research/projects/czkawka.md` | Reference Rust dedup tool; the comparison sets the bar. |
| 4 | `_research/projects/photoprism-self-hosted.md` | The PhotoPrism feature set establishes user expectations for "AI photo manager" — auto-tag + dedup are baseline. |
| 5 | `_research/projects/immich.md` | Immich's CLIP-based features are the direct comparison; the project's local-first pure-Rust stack is the differentiator. |
| 6 | `_research/projects/hydrus.md` | Hydrus's third-party `wd-e621-hydrus-tagger` shows the auto-tagging pattern with ML — the project achieves the same with no third-party tool. |
| 7 | `_research/projects/eagle-app.md` | Commercial reference — Eagle has tagging + Pinterest-Visual-Search; Image Browser delivers the same on OSS Tauri stack. |
| 8 | `_research/papers/pinterest-visual-search.md` | Pinterest's "unified visual embeddings" is the design pattern: one embedding space, multiple derived features. |
| 9 | `_research/notes` (vault) — `Roadmap.md` lines 71-72 | The user's own roadmap explicitly lists "Dedup detection" + "Auto-tagging via CLIP zero-shot classification" as M6 items. |
| 10 | `_research/notes` (vault) — `Architecture.md` | The 8 existing Tauri commands include `create_tag` and `add_tag_to_image` — the schema is already in place; auto-tagging just calls these. |

## Coupling-grade classification

**Plug-and-play.** Both features are new free functions over the existing encoder + index. They register as additional Tauri commands and add new UI panels. The existing tag system (commits `461a7da`, `a66d1f7`, `56990b7`) already has the schema + UI affordances for tag CRUD; auto-tagging just calls `add_tag_to_image` for each image × matched-label pair. Removing the rec deletes the new commands + panels; the manual tag system is unaffected.

## Integration plan

**The project today is a local-first Tauri 2 desktop app for browsing and semantically searching local image libraries with CLIP via ONNX Runtime, with manual tag CRUD + tag filter (AND/OR semantics) + 7-tier similarity retrieval.** This rec adds two automated pathways that derive new functionality from the existing CLIP embeddings + tag infrastructure. Manual tags stay; auto-tags are differentiated by an `auto_assigned: bool` column on `images_tags`.

```
   Existing primitives (preserved)        Derived features (new, Rec-11)
   ──────────────────────────────         ───────────────────────────────
   ImageEncoder (CLIP/SigLIP)             ┌─► auto_tag_images
   TextEncoder (multilingual CLIP)        │     • encode label dictionary
   VectorIndex.search()                   │     • cosine vs each image
                                          │     • assign above threshold
   Tag system (CRUD, AND/OR filter)       │
   PinterestModal UI                      ├─► find_duplicates
                                          │     • for each image, search top-K
                                          │     • flag pairs cosine ≥ 0.99
                                          │     • surface in "Duplicates" panel
                                          │     • delete-or-keep affordances
                                          │
                                          └─► both register as new
                                              Tauri commands; UI surfaces
                                              are new components only.
```

The label dictionary lives at `Library/auto_tag_labels.toml` (user-editable) with the project shipping a sensible default: ~200 labels covering common photography / art / design categories. The threshold is per-label-tunable; default 0.22 (CLIP zero-shot common threshold).

## Anti-thesis

This recommendation would NOT improve the project if:

- The user prefers strictly-manual organisation (the project's tag system is currently 100% manual; some users explicitly value that). The features are opt-in via UI; off by default.
- Auto-tag noise becomes worse than no auto-tags (over-aggressive thresholds or a poor label dictionary). The `auto_assigned: bool` flag lets the user delete all auto-tags wholesale if needed.
- For libraries with very specific domain (e.g., medical imaging), the default label dictionary is irrelevant. Then the user editing the dictionary becomes the load-bearing step — that's by design.
- Dedup at cosine ≥ 0.99 finds too many false positives in a library full of artistic variations of the same scene. Tunable threshold is the answer; the user picks per-collection.

## Implementation cost

**Medium: 1-2 weeks.**

Milestones:
1. Define the default label dictionary (~200 labels, organised by category). Half-day of curation. ~½ day.
2. Implement `auto_tag_images` Tauri command: load dictionary, encode prompts (cached after first call), iterate images, cosine-vs-each-prompt, assign above threshold via existing `add_tag_to_image`. ~2 days.
3. Add `auto_assigned: bool` column on `images_tags`; migrate existing rows. ~½ day.
4. Add UI affordance: "Auto-tag all" button in Settings drawer; per-image "auto-tag this" in PinterestModal. ~1 day.
5. Implement `find_duplicates` Tauri command: iterate images, search top-K via `VectorIndex`, return pairs above threshold. ~1 day.
6. Add UI panel: a "Duplicates" view showing pairs, with side-by-side preview + per-image keep/delete buttons. ~2 days.
7. Add config sections for both features in `Library/config.toml` + `Library/auto_tag_labels.toml`. ~½ day.
8. Integration tests + a sample-library walkthrough showing both features in action. ~1 day.
9. Update `context/systems/tag-system.md` and add `context/systems/auto-tagging-and-dedup.md`. ~½ day.

Required reading before starting: `Roadmap.md` lines 71-72 (the user's existing intent), `context/systems/tag-system.md` for the existing schema + UI patterns, `context/notes/random-shuffle-as-feature.md` for how the project thinks about UX-shaped retrieval choices.
