# Notes

Project-level rationale, conventions, and durable lessons. One bullet per note file; full content in the linked file.

## Active work areas

The project last shipped on 2026-03-04 and has been dormant since. The closest-to-active areas (where the next session is likeliest to land) are: closing the README/code truth gaps (folder picker, slideshow mode, tag deletion), the foundation pass (tracing, path normalisation, dead-code sweep), and the CLIP preprocessing quality fixes. See [Image Browser/Roadmap in LifeOS] and the `Planned / Missing / Likely Changes` sections of the relevant system files.

## Index

- [local-first-philosophy](notes/local-first-philosophy.md) — every byte stays on the user's machine; rationale for ONNX over Embeddings API, SQLite over Postgres, Tauri over Electron-with-server.
- [clip-preprocessing-decisions](notes/clip-preprocessing-decisions.md) — `FilterType::Nearest` and ImageNet-stats are intentional shortcuts with one-minute-fix paths to CLIP-native preprocessing.
- [conventions](notes/conventions.md) — `[Backend]` logging, Mutex acquire-then-execute, IPC error stringification, optimistic mutation pattern, `rand::rng()`, naming, file organisation.
- [path-and-state-coupling](notes/path-and-state-coupling.md) — why `populate_from_db` opens its own DB connection and why `normalize_path` is triplicated; the normalise-at-insert path forward.
- [random-shuffle-as-feature](notes/random-shuffle-as-feature.md) — the catalog shuffle and diversity-pool sampler are intentional UX, not bugs to fix.
- [dead-code-inventory](notes/dead-code-inventory.md) — frontend dead components (FullscreenImage, MasonrySelectedFrame, MasonryItemSelected), unused hooks (useMeasure), unused npm deps (zustand, atropos), and the `db.delete_tag` orphaned method.
- [mutex-poisoning](notes/mutex-poisoning.md) — three long-lived Mutex singletons; current posture is "panic kills the session"; `parking_lot::Mutex` is a strict upgrade when this bites.
