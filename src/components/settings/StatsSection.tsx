import { useEffect, useState } from "react";
import { getPipelineStats, type PipelineStats } from "../../services/stats";
import { Section } from "./controls";

/**
 * Pipeline progress stats — counts of images at each stage of the
 * indexing pipeline. Lets the user see at a glance how much of the
 * library has been indexed (thumbnails generated, embeddings computed).
 *
 * Polls every 5 seconds while the drawer is open. The query is a
 * single SELECT on the backend so the polling cost is negligible
 * regardless of library size.
 *
 * Why this exists: when indexing is in flight, the user has no way to
 * know how many of their images already have thumbnails vs how many
 * are still in the queue. The status pill shows aggregate progress
 * during a single pipeline run; this section shows the persistent
 * state of the index.
 */
export function StatsSection() {
  const [stats, setStats] = useState<PipelineStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const fetchOnce = async () => {
      try {
        const s = await getPipelineStats();
        if (!cancelled) {
          setStats(s);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    };
    fetchOnce();
    const interval = setInterval(fetchOnce, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  if (error) {
    return (
      <Section title="Indexing progress">
        <p className="text-xs text-destructive">{error}</p>
      </Section>
    );
  }

  if (!stats) {
    return (
      <Section title="Indexing progress">
        <p className="text-xs text-muted-foreground">Loading…</p>
      </Section>
    );
  }

  // Compute percentages for the bars. Avoid division by zero on an
  // empty library (shows "0 images" + 0% bars).
  const total = stats.total_images;
  const thumbPct = total > 0 ? Math.round((stats.with_thumbnail / total) * 100) : 0;

  // Friendly display names for each encoder. Matches the EncoderInfo
  // display_names from src-tauri/src/commands/encoders.rs but without
  // an extra IPC round-trip.
  const encoderLabel = (id: string): string => {
    switch (id) {
      case "clip_vit_b_32":
        return "CLIP";
      case "siglip2_base":
        return "SigLIP-2";
      case "dinov2_small":
        return "DINOv2";
      default:
        return id;
    }
  };

  return (
    <Section title="Indexing progress">
      <p className="text-xs text-muted-foreground -mt-1">
        Snapshot of how many images have been processed at each stage.
        Refreshes every 5 seconds.
      </p>

      <div className="space-y-3">
        <StatRow label="Total images" value={stats.total_images} />
        <ProgressRow
          label="Thumbnails"
          done={stats.with_thumbnail}
          total={total}
          pct={thumbPct}
        />
        {/* Per-encoder progress — one row per encoder. Encoders that
            haven't been indexed yet show 0/total in muted style; full
            encoders show the bar at 100%. */}
        {stats.with_embedding_per_encoder.map((ec) => {
          const pct = total > 0 ? Math.round((ec.count / total) * 100) : 0;
          return (
            <ProgressRow
              key={ec.encoder_id}
              label={`Embeddings · ${encoderLabel(ec.encoder_id)}`}
              done={ec.count}
              total={total}
              pct={pct}
            />
          );
        })}
        {stats.orphaned > 0 && (
          <StatRow
            label="Orphaned (file deleted on disk)"
            value={stats.orphaned}
            tone="warn"
          />
        )}
      </div>
    </Section>
  );
}

function StatRow({
  label,
  value,
  tone = "default",
}: {
  label: string;
  value: number;
  tone?: "default" | "warn";
}) {
  const labelClass =
    tone === "warn" ? "text-amber-500 dark:text-amber-400" : "text-foreground";
  return (
    <div className="flex items-center justify-between text-xs">
      <span className={labelClass}>{label}</span>
      <span className="tabular-nums font-medium">{value.toLocaleString()}</span>
    </div>
  );
}

function ProgressRow({
  label,
  done,
  total,
  pct,
}: {
  label: string;
  done: number;
  total: number;
  pct: number;
}) {
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <span className="text-foreground">{label}</span>
        <span className="text-muted-foreground tabular-nums">
          {done.toLocaleString()} / {total.toLocaleString()} ({pct}%)
        </span>
      </div>
      <div className="h-1.5 w-full rounded-full bg-secondary overflow-hidden">
        <div
          className="h-full bg-primary transition-all duration-500 ease-out"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
