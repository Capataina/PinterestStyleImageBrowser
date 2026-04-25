#!/usr/bin/env python3
"""
Image Browser — recommended test corpus generator.

Downloads every current League of Legends champion splash art via Riot's
public Data Dragon CDN, into a target folder you can then point the
Image Browser at via the in-app "Choose folder" button. This is the
canonical way to populate a known-good ~1500-image library so that
behaviour and search quality can be compared across machines and
contributors — i.e. when you change something and want to reproduce
the same baseline that the original developer was looking at, run this
script first.

Output:
    Default: ~/Documents/Splash Arts/    (override with --output)
    ~1500 JPG files at 1280x720, total ~3-4 GB
    Filename pattern: {ChampionName}_{SkinNumber}_{slugified-skin-name}.jpg
    Examples: Ahri_0_default.jpg, Aatrox_15_OdysseyAatrox.jpg

Usage:
    python3 scripts/download_lol_splashes.py
    python3 scripts/download_lol_splashes.py --output ~/test-images/lol-splashes

Properties of this corpus that make it useful for testing the project:
    - Every image is visually distinctive, no near-duplicates
    - Strong stylistic clusters that CLIP embeddings cluster cleanly
      (cyberpunk skins, dark fantasy, mecha, neon, lunar, animal, etc.)
    - Mix of human/character/landscape content for varied semantic tests
    - Resolution and JPEG encoding are consistent — isolates search-
      quality observations from preprocessing variance
    - All images are produced by Riot's marketing team to a unified
      style bar, but vary widely in subject — ideal for similarity-
      search relevance evaluation

How Data Dragon works (briefly, so you can extend if you want to fetch
loading splashes, tile splashes, or champion icons too):

    1. https://ddragon.leagueoflegends.com/api/versions.json
       returns the patch versions array, newest first.

    2. https://ddragon.leagueoflegends.com/cdn/{VERSION}/data/en_US/champion.json
       returns the champion catalogue keyed by PascalCase name
       (e.g. "Ahri", "MissFortune", "KSante").

    3. https://ddragon.leagueoflegends.com/cdn/{VERSION}/data/en_US/champion/{Name}.json
       returns per-champion detail with a `skins` array. Each skin has
       `num` (note: gaps when Riot retires legacy skins, so iterate the
       actual values not 0..N) and `name`.

    4. https://ddragon.leagueoflegends.com/cdn/img/champion/splash/{Name}_{N}.jpg
       is the splash itself. 1280x720 JPEG. No auth, no API key, no rate
       limiting beyond ordinary CDN politeness. Higher-resolution variants
       exist via Community Dragon (raw.communitydragon.org) but they are
       not uniform across champion vintage and inflate the corpus to
       30-50 GB — DDragon's 720p is the right call for a uniform test set.

Pure stdlib only — no aiohttp, no requests, runs on any reasonably modern
Python 3.9+ install. Resumable: re-running skips files already on disk.
"""

from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import pathlib
import re
import sys
import urllib.error
import urllib.request
from typing import Any

DDRAGON = "https://ddragon.leagueoflegends.com"
LANG = "en_US"
PARALLEL_DOWNLOADS = 8
RETRY_ON_NETWORK_ERROR = 1
USER_AGENT = (
    "image-browser-test-corpus/1.0 "
    "(github.com/Capataina/PinterestStyleImageBrowser)"
)
DEFAULT_OUTPUT_DIR = pathlib.Path.home() / "Documents" / "Splash Arts"


def fetch_json(url: str) -> Any:
    """GET a URL and parse it as JSON."""
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def slugify_skin_name(name: str) -> str:
    """Clean a skin name into a filesystem-safe segment.

    Skin names occasionally contain `/`, `:`, or unicode oddities (Riot's
    catalogue includes things like "K/DA ALL OUT Ahri Prestige Edition").
    We strip non-alphanumerics rather than encoding them; the slug is for
    a human-scannable filename, not a stable identifier.
    """
    return re.sub(r"[^A-Za-z0-9]+", "", name) or "unnamed"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Download every current League of Legends champion splash "
            "art into a folder, suitable as a test corpus for the "
            "Image Browser project."
        )
    )
    parser.add_argument(
        "--output",
        "-o",
        type=pathlib.Path,
        default=DEFAULT_OUTPUT_DIR,
        help=(
            f"Destination folder. Default: {DEFAULT_OUTPUT_DIR}. "
            f"Created if it doesn't exist."
        ),
    )
    parser.add_argument(
        "--workers",
        "-j",
        type=int,
        default=PARALLEL_DOWNLOADS,
        help=(
            f"Parallel downloads. Default: {PARALLEL_DOWNLOADS}. Increase "
            f"on a fast connection; the CDN can handle ~32 cleanly."
        ),
    )
    return parser.parse_args()


def download_one(url: str, dest: pathlib.Path) -> tuple[str, str]:
    """Download a single URL to a destination path.

    Returns (status, message) where status is one of:
        "downloaded" — file was fetched and written
        "skipped"    — file already existed
        "missing"    — server returned 404 (skin not yet in CDN, or typo)
        "failed"     — other network error
    """
    if dest.exists():
        return ("skipped", str(dest.name))

    last_error: Exception | None = None
    for attempt in range(RETRY_ON_NETWORK_ERROR + 1):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(req, timeout=60) as response:
                # Atomic write: download to .part, rename on success. An
                # interrupted download leaves a .part behind that the next
                # run overwrites. We use the same pattern as model_download.rs.
                part = dest.with_suffix(dest.suffix + ".part")
                part.write_bytes(response.read())
                part.rename(dest)
            return ("downloaded", str(dest.name))
        except urllib.error.HTTPError as e:
            # Both 404 and 403 are "the splash file isn't there." DDragon
            # serves through Cloudfront which returns 403 (not 404) for
            # missing keys. The biggest source of these is chromas: the
            # per-champion JSON lists chromas as if they were full skins,
            # but chromas share the parent skin's splash with a runtime
            # tint and don't have their own file. Roughly 75-80% of the
            # JSON entries will 403 for this reason — completely expected.
            if e.code in (404, 403):
                kind = {404: "404 — skin not in CDN", 403: "403 — likely a chroma (shares parent splash)"}[e.code]
                return ("missing", f"{dest.name} ({kind})")
            last_error = e
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            last_error = e
        # Retry once on transient failure
        if attempt < RETRY_ON_NETWORK_ERROR:
            continue
    return ("failed", f"{dest.name} ({last_error})")


def expand_champion_skins(
    version: str,
    champion_name: str,
    champion_data: dict[str, Any],
    output_dir: pathlib.Path,
) -> list[tuple[str, pathlib.Path]]:
    """For one champion, return the (url, dest_path) pairs for every skin.

    Reads the per-champion JSON to get the actual skin list. Why we need
    this even though champion.json includes basic info: the catalogue
    JSON omits the skins array, and skin `num` values aren't sequential
    (Riot retires legacy skins, leaving gaps).

    `champion_data` is unused right now but accepted so the call site
    can pass the catalogue entry without conditional logic — handy if a
    future change wants to use the numeric `key` field, e.g. to fall
    back to Community Dragon when a DDragon splash is missing.
    """
    _ = champion_data  # see docstring
    detail_url = (
        f"{DDRAGON}/cdn/{version}/data/{LANG}/champion/{champion_name}.json"
    )
    detail = fetch_json(detail_url)["data"][champion_name]
    out: list[tuple[str, pathlib.Path]] = []
    for skin in detail["skins"]:
        num = skin["num"]
        slug = slugify_skin_name(skin["name"])
        url = f"{DDRAGON}/cdn/img/champion/splash/{champion_name}_{num}.jpg"
        dest = output_dir / f"{champion_name}_{num}_{slug}.jpg"
        out.append((url, dest))
    return out


def main() -> int:
    args = parse_args()
    output_dir: pathlib.Path = args.output.expanduser().resolve()
    parallel: int = args.workers

    output_dir.mkdir(parents=True, exist_ok=True)
    print(f"Output directory: {output_dir}")
    print(f"Parallel downloads: {parallel}")

    print("Fetching latest patch version...")
    versions = fetch_json(f"{DDRAGON}/api/versions.json")
    version = versions[0]
    print(f"  Latest version: {version}")

    print("Fetching champion catalogue...")
    catalogue = fetch_json(
        f"{DDRAGON}/cdn/{version}/data/{LANG}/champion.json"
    )["data"]
    champion_names = sorted(catalogue.keys())
    print(f"  {len(champion_names)} champions")

    # Phase 1: enumerate every (url, dest) pair we need to download. This
    # is N HTTP fetches for the per-champion JSONs — done in parallel —
    # before any splash bytes are pulled.
    print(f"\nEnumerating skins ({parallel} champions in parallel)...")
    download_targets: list[tuple[str, pathlib.Path]] = []
    with cf.ThreadPoolExecutor(max_workers=parallel) as pool:
        futures = {
            pool.submit(
                expand_champion_skins,
                version,
                name,
                catalogue[name],
                output_dir,
            ): name
            for name in champion_names
        }
        for fut in cf.as_completed(futures):
            name = futures[fut]
            try:
                pairs = fut.result()
                download_targets.extend(pairs)
                print(f"  {name}: {len(pairs)} skins")
            except Exception as e:
                print(f"  {name}: FAILED to enumerate ({e})", file=sys.stderr)

    total = len(download_targets)
    print(f"\nTotal skins to consider: {total}")

    # Phase 2: parallel splash downloads. Counters are kept in main thread
    # because Python's GIL makes int increments effectively atomic for our
    # purposes; we don't need a lock here.
    print(f"\nDownloading splash arts ({parallel} in parallel, will skip already-present files)...")
    counters = {"downloaded": 0, "skipped": 0, "missing": 0, "failed": 0}
    with cf.ThreadPoolExecutor(max_workers=parallel) as pool:
        futures = [
            pool.submit(download_one, url, dest) for url, dest in download_targets
        ]
        for i, fut in enumerate(cf.as_completed(futures), start=1):
            try:
                status, msg = fut.result()
                counters[status] += 1
                marker = {
                    "downloaded": "✓",
                    "skipped": "·",
                    "missing": "?",
                    "failed": "✗",
                }[status]
                print(f"  [{i:>4}/{total}] {marker} {msg}")
            except Exception as e:
                counters["failed"] += 1
                print(f"  [{i:>4}/{total}] ✗ unexpected error: {e}", file=sys.stderr)

    print()
    print("=" * 60)
    print(f"  Downloaded:  {counters['downloaded']}")
    print(f"  Skipped:     {counters['skipped']} (already on disk)")
    print(f"  Missing:     {counters['missing']} (404/403 from CDN — mostly chromas, expected)")
    print(f"  Failed:      {counters['failed']} (network errors — these are real problems)")
    print("=" * 60)
    print(f"\nDone. Files are in: {output_dir}")
    print("Point the Image Browser at this folder via the Choose folder button.")

    return 0 if counters["failed"] == 0 else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        # Ctrl-C produces clean exit without a giant Python stack trace.
        # Already-downloaded files persist; re-running picks up where we
        # left off via the dest.exists() check.
        print("\nInterrupted. Files downloaded so far are kept; re-run to continue.")
        sys.exit(130)
