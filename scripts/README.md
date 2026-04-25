# scripts/

Helper scripts for working with the Image Browser project. None of these are required to run or build the app — they're conveniences.

## `download_lol_splashes.py` — recommended test corpus generator

Downloads every current League of Legends champion splash art via Riot's public Data Dragon CDN into a folder you can point the Image Browser at. The result is a known-good ~1500-image library, identical for every contributor and reviewer, which makes search-quality observations comparable across machines and over time.

**Why this corpus.** Splash arts are visually distinctive (no near-duplicates), span strong stylistic clusters that CLIP embeddings separate cleanly (cyberpunk, dark fantasy, mecha, neon, lunar, animal-themed), are produced by Riot to a unified style bar, and have unambiguous content tags built into the filename. They make every part of the app — masonry layout, similarity search, multilingual semantic search — easy to inspect by eye.

### Usage

```bash
# Default: downloads to ~/Documents/Splash Arts/
python3 scripts/download_lol_splashes.py

# Custom location:
python3 scripts/download_lol_splashes.py --output ~/test-images/lol

# More parallelism on a fast connection (CDN tolerates ~32 cleanly):
python3 scripts/download_lol_splashes.py --workers 32
```

### Properties

| Property | Value |
|----------|-------|
| Total files | ~1500 (varies as Riot adds champions / skins) |
| Total size | ~3-4 GB |
| Resolution | 1280×720 (uniform — DDragon's only public size) |
| Format | JPEG |
| Filename pattern | `{ChampionName}_{SkinNumber}_{slugified-skin-name}.jpg` |
| Examples | `Ahri_0_default.jpg`, `Aatrox_15_OdysseyAatrox.jpg`, `KDA_AllOut_Ahri_PrestigeEdition.jpg` |
| Dependencies | Pure Python 3.9+ stdlib — no `requests`, no `aiohttp` |
| Network | Public CDN, no auth, no API key |
| Resumable | Yes — re-running skips files already on disk |

### Why DDragon and not Community Dragon Raw

Community Dragon serves higher-resolution variants (true 4K for newer champions, 1080p-1440p for mid-era, ~720p for older) but resolution is non-uniform across champion vintage and the corpus inflates to 30-50 GB. DDragon's 720p uniformity is the right choice for a controlled test set — you can compare results across runs without resolution variance acting as a confound.

### Reproducing a specific run

The script always pulls the latest patch version. Splashes for previously-released skins are stable; only newly-added skins or rendered-update champions change between patches. If you need to pin a specific patch (rare), edit the `versions[0]` line in `main()` to a literal version string (e.g. `"15.8.1"`).

### Pointing the app at the corpus

Once the download completes:

1. Launch the app with `npm run tauri dev` from the repo root.
2. Click the "Choose folder" button next to the search bar.
3. Pick whatever directory you passed to `--output` (default: `~/Documents/Splash Arts`).
4. The status pill in the top-right of the app surfaces indexing progress: scan → first-launch model download (one time, ~1 GB from Hugging Face) → thumbnail generation → CLIP embedding. The grid populates progressively as thumbnails land.

After indexing completes, search queries to try:

- `cyberpunk` / `neon` — should surface PROJECT skins, Pulsefire variants, hextech
- `dark fantasy` — Aatrox's Darkin variants, Mordekaiser, Demacia Vice
- `lunar` — Lunar Revel skins (Annie, Diana, Aatrox)
- `mecha` — Mecha Aatrox, Mecha Kingdoms, Battle Cast
- Multilingual: `龍` (dragon), `星` (star), `чёрный` (black) — the multilingual CLIP encoder produces sensible matches in 50+ languages
- Click any tile → "More like this" — should cluster visually similar splash arts
