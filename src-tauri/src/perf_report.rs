//! On-exit profiling report renderer.
//!
//! Consumes the `timeline.jsonl` written by `perf::flush_to_file`
//! during a `--profile` session, plus the live in-memory aggregate
//! snapshot, and produces a human-readable `report.md` + a machine-
//! readable `raw.json` in the same session directory.
//!
//! Why both formats?
//!  - `report.md` is what the user actually reads — narrative, sorted,
//!    correlated. It's the primary deliverable of a profiling run.
//!  - `raw.json` is the aggregate snapshot at exit, useful for
//!    diffing two sessions programmatically without re-parsing the
//!    timeline (which can be big).
//!  - `timeline.jsonl` (already on disk by the time we render) is the
//!    full event stream for anyone who wants to write custom analysis.
//!
//! The renderer is deliberately simple and pure: take a path, read it,
//! produce the strings, write them. No global state. No allocations
//! beyond what the report itself needs. Tests cover the section
//! builders independently.

use std::cmp::Ordering;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use crate::perf::{self, RawEvent};

/// Window after a user action during which subsequent span events are
/// considered "caused by" that action. 500ms is a generous human-scale
/// threshold — anything slower than that and the user has already
/// stopped attributing the lag to their last click.
const CORRELATION_WINDOW_MS: u64 = 500;

/// How many longest-single-events to surface in the outlier section.
/// 20 is a number you can scan visually without paginating.
const OUTLIER_LIMIT: usize = 20;

/// How many spans to surface in the per-section top-N tables.
const TOP_N: usize = 15;

/// Render the markdown report + JSON snapshot for the active session.
///
/// `session_dir` should point at `Library/exports/perf-{unix_ts}/`,
/// which already contains `timeline.jsonl` populated by the running
/// flush thread. We force one final flush before reading so any
/// events buffered in the last <5s land on disk.
pub fn render_session_report(session_dir: &Path) -> io::Result<()> {
    let timeline_path = session_dir.join("timeline.jsonl");
    let report_path = session_dir.join("report.md");
    let raw_path = session_dir.join("raw.json");

    // Final flush so the timeline contains everything up to "now".
    // If the flush thread is mid-iteration this is harmless — both
    // calls drain under the same mutex.
    let _ = perf::flush_to_file(&timeline_path);

    let events = read_timeline(&timeline_path).unwrap_or_default();
    let snap = perf::snapshot();

    let report = build_markdown(&events, &snap);
    fs::write(&report_path, report)?;

    let raw_json = serde_json::to_string_pretty(&snap)
        .map_err(io::Error::other)?;
    fs::write(&raw_path, raw_json)?;

    Ok(())
}

/// Read the JSONL timeline file, skipping any malformed lines (which
/// shouldn't happen in practice but we don't want one bad row to
/// kill the whole report).
fn read_timeline(path: &Path) -> io::Result<Vec<RawEvent>> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut events = Vec::new();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<RawEvent>(&line) {
            events.push(event);
        }
    }
    Ok(events)
}

/// Top-level entry point: assemble the full markdown report from
/// the timeline + aggregate snapshot.
fn build_markdown(events: &[RawEvent], snap: &perf::PerfSnapshot) -> String {
    // Pre-sort by timestamp once. Span events arrive in close order,
    // user events in call order; both share the relative-ms clock
    // so a single sort is well-defined.
    let mut sorted = events.to_vec();
    sorted.sort_by_key(event_ts);

    let mut out = String::with_capacity(8192);
    out.push_str(&section_header(&sorted, snap));
    out.push_str(&section_top_by_total(snap));
    out.push_str(&section_hotspots(snap));
    out.push_str(&section_outliers(&sorted));
    out.push_str(&section_action_timeline(&sorted));
    out.push_str(&section_per_span_table(snap));
    out.push_str(&section_diagnostics(&sorted));
    out.push_str(&section_footer());
    out
}

fn event_ts(e: &RawEvent) -> u64 {
    match e {
        RawEvent::Span { ts_ms, .. } => *ts_ms,
        RawEvent::User { ts_ms, .. } => *ts_ms,
        RawEvent::Diagnostic { ts_ms, .. } => *ts_ms,
    }
}

fn event_is_span(e: &RawEvent) -> bool {
    matches!(e, RawEvent::Span { .. })
}

fn event_is_user(e: &RawEvent) -> bool {
    matches!(e, RawEvent::User { .. })
}

// ---------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------

fn section_header(events: &[RawEvent], snap: &perf::PerfSnapshot) -> String {
    let session_ms = events.last().map(event_ts).unwrap_or(0);
    let user_count = events.iter().filter(|e| event_is_user(e)).count();
    let span_count = events.iter().filter(|e| event_is_span(e)).count();

    // Slowest single span event, if any.
    let slowest = events
        .iter()
        .filter_map(|e| match e {
            RawEvent::Span {
                ts_ms,
                name,
                duration_us,
            } => Some((*ts_ms, name.clone(), *duration_us)),
            _ => None,
        })
        .max_by_key(|(_, _, d)| *d);

    // Total wall-clock time spent inside instrumented code, summed
    // from aggregates (more accurate than summing the raw events
    // because the raw log might have been capped/truncated).
    let total_us: f64 = snap
        .spans
        .iter()
        .map(|s| s.mean_us * (s.count as f64))
        .sum();

    let mut s = String::new();
    s.push_str("# Profiling Session Report\n\n");
    s.push_str(&format!(
        "- **Session duration:** {}\n",
        format_ms_human(session_ms)
    ));
    s.push_str(&format!(
        "- **Total time inside instrumented code:** {}\n",
        format_us_human(total_us as u64)
    ));
    s.push_str(&format!("- **Span events recorded:** {span_count}\n"));
    s.push_str(&format!("- **User actions recorded:** {user_count}\n"));
    if let Some((ts_ms, name, dur)) = slowest {
        s.push_str(&format!(
            "- **Slowest single event:** `{name}` at t={} took {}\n",
            format_ms_human(ts_ms),
            format_us_human(dur)
        ));
    }
    s.push('\n');
    s
}

fn section_top_by_total(snap: &perf::PerfSnapshot) -> String {
    let mut rows: Vec<(String, u64, f64, f64, f64, f64)> = snap
        .spans
        .iter()
        .map(|sp| {
            let total_us = sp.mean_us * (sp.count as f64);
            (
                sp.name.clone(),
                sp.count,
                total_us,
                sp.mean_us,
                sp.p95_us,
                sp.max_us,
            )
        })
        .collect();
    rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));
    rows.truncate(TOP_N);

    let mut s = String::new();
    s.push_str(&format!(
        "## Top {TOP_N} spans by total time consumed\n\n"
    ));
    s.push_str("Wallclock time spent in each span over the whole session. \
                The thing at the top is where the app actually spent its time — \
                optimise here for the biggest absolute wins.\n\n");
    s.push_str("| # | Span | n | total | mean | p95 | max |\n");
    s.push_str("|---|------|---|-------|------|-----|-----|\n");
    if rows.is_empty() {
        s.push_str("| - | _(no spans recorded)_ | - | - | - | - | - |\n");
    }
    for (i, (name, n, total, mean, p95, max)) in rows.iter().enumerate() {
        s.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} |\n",
            i + 1,
            name,
            n,
            format_us_human(*total as u64),
            format_us_human(*mean as u64),
            format_us_human(*p95 as u64),
            format_us_human(*max as u64),
        ));
    }
    s.push('\n');
    s
}

fn section_hotspots(snap: &perf::PerfSnapshot) -> String {
    // Sort by p95 descending — the things that occasionally take
    // forever, even if their mean is fine.
    let mut rows: Vec<&perf::SpanSnapshot> = snap.spans.iter().collect();
    rows.sort_by(|a, b| b.p95_us.partial_cmp(&a.p95_us).unwrap_or(Ordering::Equal));
    rows.truncate(TOP_N);

    let mut s = String::new();
    s.push_str(&format!("## Top {TOP_N} hotspots by p95\n\n"));
    s.push_str("Spans whose 95th-percentile latency is highest. \
                Look here for tail-latency issues — things that are usually fast \
                but occasionally stall.\n\n");
    s.push_str("| # | Span | n | p50 | p95 | p99 | max |\n");
    s.push_str("|---|------|---|-----|-----|-----|-----|\n");
    if rows.is_empty() {
        s.push_str("| - | _(no spans recorded)_ | - | - | - | - | - |\n");
    }
    for (i, sp) in rows.iter().enumerate() {
        s.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} |\n",
            i + 1,
            sp.name,
            sp.count,
            format_us_human(sp.p50_us as u64),
            format_us_human(sp.p95_us as u64),
            format_us_human(sp.p99_us as u64),
            format_us_human(sp.max_us as u64),
        ));
    }
    s.push('\n');
    s
}

fn section_outliers(sorted: &[RawEvent]) -> String {
    // Collect every span with its preceding user action (within the
    // correlation window). Sort by duration descending, take top N.
    let mut spans: Vec<(u64, &str, u64, Option<&str>)> = Vec::new();
    let mut last_user: Option<(u64, &str)> = None;
    for e in sorted {
        match e {
            RawEvent::User { ts_ms, action, .. } => {
                last_user = Some((*ts_ms, action.as_str()));
            }
            RawEvent::Span {
                ts_ms,
                name,
                duration_us,
            } => {
                let cause = last_user.and_then(|(uts, ua)| {
                    if ts_ms.saturating_sub(uts) <= CORRELATION_WINDOW_MS {
                        Some(ua)
                    } else {
                        None
                    }
                });
                spans.push((*ts_ms, name.as_str(), *duration_us, cause));
            }
            // Diagnostic events don't appear in the outlier table —
            // they're rendered in the dedicated Diagnostics section.
            RawEvent::Diagnostic { .. } => {}
        }
    }
    // 6b — clippy::unnecessary_sort_by. Reverse-key sort_by_key is the
    // idiomatic descending sort.
    spans.sort_by_key(|b| std::cmp::Reverse(b.2));
    spans.truncate(OUTLIER_LIMIT);

    let mut s = String::new();
    s.push_str(&format!(
        "## Outlier events (top {OUTLIER_LIMIT} longest)\n\n"
    ));
    s.push_str(&format!(
        "The single slowest spans of the session. \
         The \"triggered by\" column shows the most recent user action that \
         fired within {CORRELATION_WINDOW_MS}ms before the span — that's \
         your prime suspect.\n\n"
    ));
    s.push_str("| # | t | Span | Duration | Triggered by |\n");
    s.push_str("|---|---|------|----------|---------------|\n");
    if spans.is_empty() {
        s.push_str("| - | - | _(no span events recorded)_ | - | - |\n");
    }
    for (i, (ts, name, dur, cause)) in spans.iter().enumerate() {
        s.push_str(&format!(
            "| {} | {} | `{}` | {} | {} |\n",
            i + 1,
            format_ms_human(*ts),
            name,
            format_us_human(*dur),
            cause.map(|c| format!("`{c}`")).unwrap_or_else(|| "—".into()),
        ));
    }
    s.push('\n');
    s
}

fn section_action_timeline(sorted: &[RawEvent]) -> String {
    // For each user action, list the span events that fired in the
    // correlation window after it. Long sessions might produce a lot
    // of output; we cap at 50 user actions to keep the report
    // scannable, and within each action we cap at 10 spans.
    let user_events: Vec<(u64, &str, &serde_json::Value)> = sorted
        .iter()
        .filter_map(|e| match e {
            RawEvent::User {
                ts_ms,
                action,
                payload,
            } => Some((*ts_ms, action.as_str(), payload)),
            _ => None,
        })
        .collect();

    let mut s = String::new();
    s.push_str("## Action timeline (correlated)\n\n");
    s.push_str(&format!(
        "Each user action followed by the span events it triggered \
         (within {CORRELATION_WINDOW_MS}ms). Read top-to-bottom to replay \
         the session.\n\n"
    ));

    if user_events.is_empty() {
        s.push_str("_(no user actions recorded)_\n\n");
        return s;
    }

    let cap = 50usize;
    let truncated = user_events.len() > cap;
    let to_show = user_events.iter().take(cap);

    for (uts, action, payload) in to_show {
        s.push_str(&format!(
            "### t={} · `{}` {}\n\n",
            format_ms_human(*uts),
            action,
            format_payload(payload)
        ));
        // Find span events that started in the next CORRELATION_WINDOW_MS.
        // The events were recorded on close, so ts_ms is the close
        // time, not the start; we approximate by treating ts_ms as
        // "the moment we knew about the span" which is good enough
        // for human-scale correlation.
        let causal: Vec<&RawEvent> = sorted
            .iter()
            .filter(|e| match e {
                RawEvent::Span { ts_ms, .. } => {
                    *ts_ms >= *uts && *ts_ms - *uts <= CORRELATION_WINDOW_MS
                }
                _ => false,
            })
            .take(10)
            .collect();

        if causal.is_empty() {
            s.push_str("_(no span events in the correlation window)_\n\n");
        } else {
            for e in causal {
                if let RawEvent::Span {
                    ts_ms,
                    name,
                    duration_us,
                } = e
                {
                    s.push_str(&format!(
                        "- t+{}: `{}` ({})\n",
                        format_ms_human(ts_ms.saturating_sub(*uts)),
                        name,
                        format_us_human(*duration_us)
                    ));
                }
            }
            s.push('\n');
        }
    }
    if truncated {
        s.push_str(&format!(
            "_(timeline truncated — showing first {cap} of {} actions; \
             see timeline.jsonl for the full stream)_\n\n",
            user_events.len()
        ));
    }
    s
}

fn section_per_span_table(snap: &perf::PerfSnapshot) -> String {
    let mut rows: Vec<&perf::SpanSnapshot> = snap.spans.iter().collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));

    let mut s = String::new();
    s.push_str("## Per-span detail (alphabetical)\n\n");
    s.push_str("Every instrumented span observed during the session.\n\n");
    s.push_str("| Span | n | mean | p50 | p95 | p99 | max |\n");
    s.push_str("|------|---|------|-----|-----|-----|-----|\n");
    if rows.is_empty() {
        s.push_str("| _(no spans recorded)_ | - | - | - | - | - | - |\n");
    }
    for sp in rows {
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} |\n",
            sp.name,
            sp.count,
            format_us_human(sp.mean_us as u64),
            format_us_human(sp.p50_us as u64),
            format_us_human(sp.p95_us as u64),
            format_us_human(sp.p99_us as u64),
            format_us_human(sp.max_us as u64),
        ));
    }
    s.push('\n');
    s
}

/// Diagnostics section — renders structured snapshots emitted via
/// `perf::record_diagnostic`. Grouped by `diagnostic` kind so the
/// reader can scan for "search_query" results (audit cosine output)
/// or "startup_state" (audit per-encoder embedding counts).
fn section_diagnostics(sorted: &[RawEvent]) -> String {
    let mut s = String::new();
    s.push_str("## Diagnostics\n\n");

    let diagnostics: Vec<&RawEvent> = sorted
        .iter()
        .filter(|e| matches!(e, RawEvent::Diagnostic { .. }))
        .collect();

    if diagnostics.is_empty() {
        s.push_str(
            "_No diagnostic events recorded this session. Diagnostics fire for \
             search queries, encoder cache populates, and startup state — \
             trigger any of those to populate this section._\n\n",
        );
        return s;
    }

    // Group by diagnostic kind for scannability.
    use std::collections::BTreeMap;
    let mut by_kind: BTreeMap<&str, Vec<&RawEvent>> = BTreeMap::new();
    for e in &diagnostics {
        if let RawEvent::Diagnostic { diagnostic, .. } = e {
            by_kind.entry(diagnostic.as_str()).or_default().push(e);
        }
    }

    s.push_str(&format!(
        "{} diagnostic events across {} kinds. Each kind grouped below; \
         within a kind, ordered chronologically.\n\n",
        diagnostics.len(),
        by_kind.len()
    ));

    for (kind, events) in &by_kind {
        s.push_str(&format!(
            "### `{}` ({} events)\n\n",
            kind,
            events.len()
        ));
        for e in events {
            if let RawEvent::Diagnostic {
                ts_ms, payload, ..
            } = e
            {
                s.push_str(&format!(
                    "<details><summary>t={}</summary>\n\n```json\n{}\n```\n\n</details>\n\n",
                    format_ms_human(*ts_ms),
                    serde_json::to_string_pretty(payload)
                        .unwrap_or_else(|e| format!("(payload serialise failed: {e})"))
                ));
            }
        }
    }
    s
}

fn section_footer() -> String {
    let mut s = String::new();
    s.push_str("---\n\n");
    s.push_str(
        "Generated by the Pinterest Image Browser profiling system. \
         Raw event stream: `timeline.jsonl`. Aggregate snapshot at exit: \
         `raw.json`.\n",
    );
    s
}

// ---------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------

/// Format microseconds at human scale: us / ms / s / m. Picks the
/// most readable unit at each magnitude.
fn format_us_human(us: u64) -> String {
    if us < 1_000 {
        format!("{us}μs")
    } else if us < 1_000_000 {
        format!("{:.2}ms", us as f64 / 1_000.0)
    } else if us < 60_000_000 {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    } else {
        let mins = us / 60_000_000;
        let secs = (us % 60_000_000) as f64 / 1_000_000.0;
        format!("{mins}m {secs:.1}s")
    }
}

/// Format milliseconds at human scale. Same approach, different unit.
fn format_ms_human(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.2}s", ms as f64 / 1_000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) as f64 / 1_000.0;
        format!("{mins}m {secs:.1}s")
    }
}

/// Render a user-action payload object as a compact `key=value`
/// string. Empty objects render as nothing. Keeps the heading
/// readable instead of dumping inline JSON braces.
fn format_payload(payload: &serde_json::Value) -> String {
    let serde_json::Value::Object(map) = payload else {
        return String::new();
    };
    if map.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = map
        .iter()
        .map(|(k, v)| {
            let v_str = match v {
                serde_json::Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            format!("{k}={v_str}")
        })
        .collect();
    parts.sort();
    format!("({})", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_us_human_picks_appropriate_unit() {
        assert_eq!(format_us_human(500), "500μs");
        assert_eq!(format_us_human(1_500), "1.50ms");
        assert_eq!(format_us_human(2_500_000), "2.50s");
        // 2 minutes 30 seconds
        assert!(format_us_human(150_000_000).starts_with("2m"));
    }

    #[test]
    fn format_ms_human_picks_appropriate_unit() {
        assert_eq!(format_ms_human(500), "500ms");
        assert_eq!(format_ms_human(1_500), "1.50s");
        assert!(format_ms_human(150_000).starts_with("2m"));
    }

    #[test]
    fn format_payload_renders_key_value_pairs() {
        let p = json!({ "id": 42, "name": "foo" });
        let out = format_payload(&p);
        // Sorted alphabetically — id before name.
        assert_eq!(out, "(id=42, name=foo)");
    }

    #[test]
    fn format_payload_handles_empty_object() {
        assert_eq!(format_payload(&json!({})), "");
    }

    #[test]
    fn outlier_section_correlates_user_action_with_following_span() {
        let events = vec![
            RawEvent::User {
                ts_ms: 100,
                action: "image_click".into(),
                payload: json!({ "id": 42 }),
            },
            RawEvent::Span {
                ts_ms: 150,
                name: "get_similar_images".into(),
                duration_us: 5_000_000, // 5s — definitely an outlier
            },
        ];
        let report = section_outliers(&events);
        assert!(
            report.contains("`get_similar_images`"),
            "outlier section missing the slow span: {report}"
        );
        assert!(
            report.contains("`image_click`"),
            "outlier section didn't correlate to the preceding user action: {report}"
        );
    }

    #[test]
    fn outlier_section_drops_correlation_outside_window() {
        // User action at t=0, span at t=1000 — well outside the 500ms
        // window. The span should still appear, but with no cause.
        let events = vec![
            RawEvent::User {
                ts_ms: 0,
                action: "image_click".into(),
                payload: json!({}),
            },
            RawEvent::Span {
                ts_ms: 1000,
                name: "later_span".into(),
                duration_us: 100_000,
            },
        ];
        let report = section_outliers(&events);
        assert!(report.contains("`later_span`"));
        // The "triggered by" column for this row should be the
        // em-dash placeholder — we do this by checking the row line
        // directly.
        let row = report
            .lines()
            .find(|l| l.contains("`later_span`"))
            .expect("later_span row missing");
        assert!(
            row.contains("| — |"),
            "expected em-dash placeholder for out-of-window span, got: {row}"
        );
    }

    #[test]
    fn build_markdown_includes_every_section_header() {
        let events = vec![RawEvent::User {
            ts_ms: 100,
            action: "test".into(),
            payload: json!({}),
        }];
        let snap = perf::PerfSnapshot {
            spans: vec![],
            timestamp: 0,
        };
        let md = build_markdown(&events, &snap);
        assert!(md.contains("# Profiling Session Report"));
        assert!(md.contains("## Top "));
        assert!(md.contains("## Outlier events"));
        assert!(md.contains("## Action timeline"));
        assert!(md.contains("## Per-span detail"));
    }

    #[test]
    fn read_timeline_returns_empty_when_file_missing() {
        let path = std::env::temp_dir().join(format!(
            "missing_perf_{}.jsonl",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let out = read_timeline(&path).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn read_timeline_skips_malformed_lines() {
        let dir = std::env::temp_dir().join(format!(
            "perf_malformed_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("timeline.jsonl");
        fs::write(
            &path,
            "this is not json\n\
             {\"kind\":\"span\",\"ts_ms\":1,\"name\":\"valid\",\"duration_us\":42}\n\
             \n\
             {\"kind\":\"oops\"}\n",
        )
        .unwrap();
        let out = read_timeline(&path).unwrap();
        assert_eq!(out.len(), 1, "should keep only the one valid event");
        let _ = fs::remove_dir_all(&dir);
    }

}
