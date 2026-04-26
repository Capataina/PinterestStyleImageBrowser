# Performance analysis

## Header

- **Status:** diagnostic complete; fixes pending
- **Date:** 2026-04-26
- **Primary report analysed:** `/Users/atacanercetinkaya/Library/Application Support/com.ataca.image-browser/exports/perf-1777212369/report.md`
- **Raw timeline:** `/Users/atacanercetinkaya/Library/Application Support/com.ataca.image-browser/exports/perf-1777212369/timeline.jsonl`
- **Trigger:** the app felt heavily laggy during a run after adding `/Users/atacanercetinkaya/Documents/Splash Arts`.

## Executive conclusion

The lag is not caused by normal steady-state image browsing. It appears during fresh indexing, when expensive encoder work is running while the frontend repeatedly refreshes and rerenders the full image grid.

The two main contributors are:

1. **Sustained backend resource pressure from image encoders**, especially SigLIP-2.
2. **A frontend refresh loop that invalidates the full `["images"]` query during encoding**, causing full `get_images` calls, item remapping, and Masonry rerenders while the backend is already saturated.

The visible freeze is amplified by Masonry render cost. The report shows many Masonry updates taking far longer than a frame budget, so even without the catastrophic backend stalls the grid can visibly jank.

## Evidence

### Latest report summary

The analysed report covers roughly **9m52.5s** of app runtime and **11m29.6s** of instrumented span time.

| Span / action | Count | Total | Mean | p95 | Max | Interpretation |
|---|---:|---:|---:|---:|---:|---|
| `siglip2.encode_image` | 1842 | 7m45.3s | 252.60ms | 1.49s | 2.92s | Main sustained indexing cost. |
| `clip.encode_image_batch` | 58 | 1m19.2s | 1.36s | 1.63s | 3.66s | Significant early encoder cost. |
| `thumbnail.generate` | 1842 | 34.53s | 18.75ms | n/a | n/a | Noticeable but not the main lag source. |
| `dinov2.encode_image` | 150 | 27.42s | 182.8ms | n/a | n/a | Still running/partial in this report. |
| `ipc.get_images` | 78 | 50.06s | 641.85ms | 124.73ms | 22.31s | Usually fine, but two catastrophic stalls. |
| `ipc.semantic_search` | 10 | 3.34s | 334ms | n/a | n/a | Not responsible for the long freeze. |

The slowest single event was:

```text
ipc.get_images at t ~= 8m56.9s, duration ~= 22.31s
```

There was another comparable stall:

```text
ipc.get_images at t ~= 8m8.6s, duration ~= 22.15s
```

### `get_images` is usually not slow

Around the problematic window, frontend-measured `get_images` calls looked like this:

```text
432.064s  get_images   92ms
439.766s  get_images  101ms
446.728s  get_images   93ms
453.529s  get_images  124ms
496.261s  get_images  28832ms
542.906s  get_images  27752ms
545.902s  get_images   81ms
556.165s  get_images   73ms
561.643s  get_images   78ms
```

That shape matters: the query is normally sub-125ms even during indexing, then suddenly blocks for ~28s twice, then immediately returns to normal.

Running the equivalent SQLite query after the run completed returned in roughly **58ms** over about **1989 images**. Current database state:

```text
images:      1989
embeddings: 4253
encoders:
  clip_vit_b_32: 1989
  siglip2_base: 1989
  dinov2_base:   275
```

This makes a pure SQL-complexity explanation unlikely. The more likely explanation is contention or resource starvation during indexing.

### The bad `get_images` window overlaps SigLIP slowdown

During the worst window, SigLIP-2 encode durations degraded heavily.

```text
Window 450s-545s:
  siglip2.encode_image count: 107
  total SigLIP span time:     90.22s
  >1s SigLIP events:          51

Window 545s-565s:
  siglip2.encode_image count: 119
  total SigLIP span time:     19.55s
  >1s SigLIP events:          0
```

The second 22s `get_images` stall ends just before the SigLIP slowdown clears and `get_images` returns to ~70-80ms. This is strong circumstantial evidence that the app is being starved while heavy model inference is active.

### Masonry renders are a visible jank source

React profiling showed:

```text
Masonry render/update events: 111
mean duration:                85.9ms
max duration:                 404ms
events >16ms:                 61
events >50ms:                 54
events >100ms:                39
events >200ms:                17
```

At 60Hz, a frame has about **16.7ms**. A 100-400ms render is visible as a freeze. These renders are not merely background cost; they directly affect interactivity.

## Code path implicated

The refresh loop is in `src/hooks/useIndexingProgress.ts`.

During `thumbnail` or `encode`, it invalidates the full images query roughly every 5 seconds:

```text
progress.phase === "thumbnail" || progress.phase === "encode"
  -> queryClient.invalidateQueries({ queryKey: ["images"] })
```

That drives this path:

```text
useIndexingProgress
  -> invalidate ["images"]
  -> useImages
  -> services/images.ts::fetchImages
  -> perfInvoke("get_images")
  -> backend get_images_with_thumbnails
  -> map every backend row to ImageItem
  -> convertFileSrc for image + thumbnail paths
  -> sort/filter displayImages
  -> Masonry rerender
```

The frontend is therefore asking for and rerendering the entire grid repeatedly while the backend is doing encoder work.

## What is probably not the main cause

### Not normal image query speed

The `get_images` SQL path is not inherently a 22s operation for the current library size. It is normally tens of milliseconds outside the contention window.

### Not semantic search

Semantic search totalled only ~3.34s across 10 calls. It is not the source of the long grid freeze.

### Not thumbnail generation alone

Thumbnail generation consumed ~34.53s total across 1842 images, but individual thumbnail generation was modest. The UI remained especially bad during the later encode-heavy window, not only during thumbnail generation.

### Not first-run model download in this report

An earlier report showed model download taking ~4m52s. That is a separate first-launch delay. The latest lag report is dominated by indexing and rendering, not download.

## Assumptions needing stronger evidence

- **Assumption:** the two ~22s `get_images` stalls are caused by resource starvation or lock contention while SigLIP-2 is running.
- **Why this is plausible:** normal `get_images` calls are ~70-125ms; the stalls overlap a SigLIP slowdown window; the same SQL query is ~58ms after indexing pressure is gone.
- **What would prove it:** subspans inside `get_images_with_thumbnails` for DB mutex wait, SQLite prepare/query/row iteration, aggregation, mapping/sort, and IPC serialisation.

## Failure modes and counter-scenarios

- **SQLite lock wait could be the real culprit.** If encoder writes hold a lock or starve the foreground connection, the fix needs DB scheduling/connection work, not only frontend throttling.
- **IPC serialisation could dominate at scale.** If returning ~2000 image rows through Tauri IPC sometimes blocks under webview pressure, reducing refresh frequency helps but does not fully solve the boundary cost.
- **Masonry may remain janky after backend fixes.** Even if `get_images` never stalls again, 100-400ms Masonry renders are enough to make scrolling and updates feel bad.
- **Parallelising the pipeline could make this worse.** The existing parallelism plan may improve wall-clock indexing, but if it increases simultaneous encode and UI work without throttling, it can increase contention and visible lag.

## Recommended fix order

### 1. Stop refreshing the full grid every 5 seconds during encode

Change the invalidation policy in `useIndexingProgress`.

Recommended behaviour:

- During `thumbnail`: refresh images periodically, because thumbnails are becoming available and the grid visibly benefits.
- During `encode`: do not refresh the full image grid every 5 seconds. Encoding mostly changes search readiness, not the visible grid.
- On `ready`: invalidate `["images"]` once so final metadata is fresh.

This is the cheapest high-confidence fix because it directly removes repeated full-grid work during the heaviest backend phase.

### 2. Split visible indexing from background enrichment

Treat thumbnails and one primary encoder as foreground work. Treat SigLIP-2 and DINOv2 as background enrichment unless the user explicitly needs them immediately.

Possible policy:

```text
foreground:
  scan -> thumbnails -> selected/default encoder -> ready/interactable

background:
  additional encoders -> cosine/cache refresh -> search quality improves over time
```

This preserves fast first usability while still allowing richer embeddings later.

### 3. Add `get_images` subspans before DB tuning

Do not blindly add indexes or rewrite SQL yet. First instrument:

- foreground DB connection lock acquisition
- SQL prepare time
- SQLite row iteration time
- aggregation into the `HashMap`
- map/sort into `ImageData`
- response payload size / IPC serialisation time if measurable

If lock wait dominates, tune DB scheduling. If row iteration dominates, tune SQL/indexes. If IPC dominates, reduce payload frequency/size.

### 4. Reduce Masonry render cost

The Masonry component is currently a first-order UI bottleneck.

Options:

- Virtualise the grid so only visible items render.
- Memoise item components only if profiling shows prop stability is good enough to benefit.
- Avoid re-sorting/remapping the full image list on every indexing progress refresh.
- Avoid refreshing `displayImages` unless the actual visible image set changed.

Virtualisation is the structural fix; memoisation is only useful if the render tree already has stable identities.

### 5. Throttle heavy encoders around interaction

If SigLIP-2 continues to starve the app, add a cooperative policy:

- pause or yield between SigLIP images when the user is interacting;
- avoid running additional encoders while the app is doing expensive grid work;
- consider a lower-priority worker thread or smaller encode batches where supported.

## Proposed implementation checklist

- [ ] Change `useIndexingProgress` so `encode` does not invalidate `["images"]` every 5 seconds.
- [ ] Keep one final `["images"]` invalidation on `ready`.
- [ ] Add backend subspans inside `get_images_with_thumbnails`.
- [ ] Re-run the same folder-add scenario and compare `ipc.get_images` max duration.
- [ ] Re-run the same folder-add scenario and compare Masonry render p95/max.
- [ ] Re-run the same folder-add scenario and compare total SigLIP encode time.
- [ ] Re-run the same folder-add scenario and compare the count of renders >100ms.
- [ ] Decide whether Masonry virtualisation is required after the invalidation fix.
- [ ] Decide whether additional encoders should become background enrichment rather than foreground readiness work.

## Success criteria

For a fresh folder add of roughly 2000 images:

- `ipc.get_images` should have no multi-second stalls.
- Masonry render p95 should be well under 50ms, with no routine 100-400ms updates.
- The app should remain usable while SigLIP-2/DINOv2 continue in the background.
- The report should clearly distinguish DB wait, SQL work, aggregation, and IPC cost for `get_images`.
