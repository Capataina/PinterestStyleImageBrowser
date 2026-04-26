import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useUserPreferences } from "../../hooks/useUserPreferences";
import { recordAction } from "../../services/perf";
import { Section } from "./controls";

/**
 * Module-level "last pushed" cache for the priority-encoder IPC.
 *
 * Why module-level rather than `useRef`: React 19 StrictMode mounts →
 * unmounts → re-mounts every component once in dev, AND the
 * SettingsDrawer is conditionally mounted (each open is a fresh mount).
 * A `useRef` is destroyed on every unmount, so a fresh ref starts at
 * `null` on the next mount and the dedup short-circuit never fires —
 * exactly the failure pattern the on-exit profiling report at t=5.87s
 * captured (`set_priority_image_encoder` firing twice within 13ms per
 * settings_open). A module-level cache survives both StrictMode replays
 * and conditional remounts because it lives in the JS module namespace,
 * not in React's component tree.
 *
 * Trade-off: HMR reloads of this file reset it (acceptable — HMR
 * already re-runs effects). Test isolation: `__resetEncoderPushCache`
 * is exported below so vitest's `beforeEach` can clear it without
 * reaching into module internals.
 */
let lastPushedImageEncoder: string | null = null;

/** Test-only: reset the module-level dedup cache. Not part of the
 *  public API — production code never needs to reset, the cache only
 *  matters within a single browser session. */
export function __resetEncoderPushCache() {
  lastPushedImageEncoder = null;
}

/**
 * Backend EncoderInfo, mirrors src-tauri/src/commands/encoders.rs.
 */
interface EncoderInfo {
  id: string;
  display_name: string;
  description: string;
  dim: number;
  supports_text: boolean;
  supports_image: boolean;
}

/**
 * Encoder picker — two dropdowns.
 *
 * Image → Image: which encoder to use when the user clicks an image
 * to find similar ones. DINOv2 dominates here for identity / pose /
 * art-style queries; CLIP/SigLIP are also valid for concept-level
 * "find more of this idea".
 *
 * Text → Image: which encoder to use when the user types a query.
 * SigLIP-2 has better English alignment than CLIP-multilingual; CLIP
 * is the legacy default.
 *
 * Each option carries a description shown in a hover tooltip + a
 * `<details>` expandable so the rationale is always one click away.
 *
 * Switching an encoder is INSTANT — no re-encoding, because the
 * indexing pipeline already wrote embeddings for all three encoders
 * to the embeddings table. The cosine cache reloads from disk for
 * the new encoder on the next search call (~ms for ~10k images).
 */
export function EncoderSection() {
  const { prefs, update } = useUserPreferences();
  const [encoders, setEncoders] = useState<EncoderInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<EncoderInfo[]>("list_available_encoders")
      .then((list) => {
        if (!cancelled) setEncoders(list);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Push the current image-encoder pick to the backend on mount + on
  // change. The backend reads `priority_image_encoder` from settings.json
  // at the start of every indexing pipeline run; having it match what
  // the picker shows means the next pipeline (or the next watcher
  // rescan) runs the user's encoder FIRST and hot-populates the cosine
  // cache for it as soon as that phase finishes — instead of always
  // running CLIP → SigLIP-2 → DINOv2 in fixed order.
  //
  // Idempotency via the module-level `lastPushedImageEncoder` cache
  // (declared above the component): React 19 StrictMode replays mount
  // effects in dev, AND the SettingsDrawer is conditionally mounted —
  // every drawer open re-mounts EncoderSection. A `useRef` would be
  // destroyed on each unmount and reset to null, so the next mount
  // would always re-fire the IPC; the module-level cache survives both
  // failure modes. The on-exit profiling report (settings_open at
  // t=5.87s) captured the duplicate pair `+30ms` and `+43ms`; this
  // dedupes at the IPC boundary rather than papering over with a
  // debounce so a genuine encoder change is still pushed immediately,
  // with no race window. The backend `set_priority_image_encoder`
  // command also short-circuits when the value matches the persisted
  // one — defence-in-depth so any future caller can't reintroduce the
  // settings.json churn.
  useEffect(() => {
    if (lastPushedImageEncoder === prefs.imageEncoder) return;
    const target = prefs.imageEncoder;
    // Mark BEFORE awaiting the IPC — under React 19 StrictMode the
    // mount → unmount → re-mount cycle runs SYNCHRONOUSLY, so if we
    // only set the cache on `.then()` the second mount's effect runs
    // before the first promise resolves and re-fires the IPC. Setting
    // synchronously means the second mount sees the cache populated
    // and short-circuits cleanly. Trade-off: a failed IPC leaves the
    // cache poisoned for this session; the next genuine value change
    // (user picks a different encoder) clears it, and the validation
    // effect below normalises away unknown encoder IDs anyway.
    lastPushedImageEncoder = target;
    invoke("set_priority_image_encoder", { id: target }).catch((e) => {
      // Non-fatal: an unknown id at the backend (e.g. encoder removed
      // between releases) just means the pipeline falls back to its
      // default ordering. Log so the failure isn't silent in dev.
      console.warn("set_priority_image_encoder failed:", e);
    });
  }, [prefs.imageEncoder]);

  // Validate the saved encoder choice against the live options. Cleans
  // up stale localStorage entries (e.g. the user picked SigLIP-2 in a
  // previous session, then we removed it from the available list when
  // the URL turned out to be 401-gated). Without this, picking an
  // unknown encoder ID silently produces 0 search results.
  useEffect(() => {
    if (!encoders) return;
    if (!encoders.find((e) => e.id === prefs.imageEncoder && e.supports_image)) {
      const fallback = encoders.find((e) => e.supports_image);
      if (fallback) update("imageEncoder", fallback.id);
    }
    if (!encoders.find((e) => e.id === prefs.textEncoder && e.supports_text)) {
      const fallback = encoders.find((e) => e.supports_text);
      if (fallback) update("textEncoder", fallback.id);
    }
  }, [encoders, prefs.imageEncoder, prefs.textEncoder, update]);

  if (error) {
    return (
      <Section title="Encoders">
        <p className="text-xs text-destructive">{error}</p>
      </Section>
    );
  }
  if (!encoders) {
    return (
      <Section title="Encoders">
        <p className="text-xs text-muted-foreground">Loading…</p>
      </Section>
    );
  }

  const imageOptions = encoders.filter((e) => e.supports_image);
  const textOptions = encoders.filter((e) => e.supports_text);
  const selectedImage = encoders.find((e) => e.id === prefs.imageEncoder);
  const selectedText = encoders.find((e) => e.id === prefs.textEncoder);

  return (
    <Section title="Encoders">
      <p className="text-xs text-muted-foreground -mt-1">
        Pick which model encodes images for similarity search. Switching
        is instant — embeddings for all encoders are stored on disk and
        reload from there.
      </p>

      <div className="space-y-4">
        <Picker
          label="Image → Image (View Similar)"
          value={prefs.imageEncoder}
          options={imageOptions}
          selected={selectedImage}
          onChange={(id) => {
            // Breadcrumb so the on-exit profiling report's diagnostic
            // section shows when the user switched encoders — lets us
            // correlate "search-quality complaint at t=3:45" with
            // "encoder switched from CLIP→DINOv2 at t=3:42".
            recordAction("encoder_changed", {
              field: "imageEncoder",
              from: prefs.imageEncoder,
              to: id,
            });
            update("imageEncoder", id);
          }}
        />
        <Picker
          label="Text → Image (Semantic Search)"
          value={prefs.textEncoder}
          options={textOptions}
          selected={selectedText}
          onChange={(id) => {
            recordAction("encoder_changed", {
              field: "textEncoder",
              from: prefs.textEncoder,
              to: id,
            });
            update("textEncoder", id);
          }}
        />
      </div>
    </Section>
  );
}

function Picker({
  label,
  value,
  options,
  selected,
  onChange,
  experimental,
}: {
  label: string;
  value: string;
  options: EncoderInfo[];
  selected?: EncoderInfo;
  onChange: (id: string) => void;
  experimental?: string;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium text-foreground block">
        {label}
      </label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full text-xs rounded-md border border-border bg-secondary text-foreground px-2.5 py-1.5 hover:bg-accent focus:outline-none focus:ring-2 focus:ring-primary/50"
      >
        {options.map((opt) => (
          <option key={opt.id} value={opt.id} title={opt.description}>
            {opt.display_name}
          </option>
        ))}
      </select>
      {selected && (
        <details className="text-[11px] text-muted-foreground">
          <summary className="cursor-pointer hover:text-foreground transition">
            Why pick this?
          </summary>
          <p className="mt-1 pl-3 leading-relaxed">
            {selected.description}{" "}
            <span className="text-[10px] opacity-70">
              ({selected.dim}-dim)
            </span>
          </p>
        </details>
      )}
      {experimental && (
        <p className="text-[11px] text-amber-500 dark:text-amber-400">
          {experimental}
        </p>
      )}
    </div>
  );
}
