import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { recordAction } from "../../services/perf";
import { Section } from "./controls";

/**
 * Phase 11c — per-encoder enable/disable toggles (replaces both
 * dropdowns).
 *
 * Each row is one supported encoder + a switch. Toggling persists
 * via the `set_enabled_encoders` IPC and takes effect:
 *
 *   - immediately for fusion (next `get_fused_*` IPC reads from
 *     settings.json)
 *   - on the next indexing pipeline run for encoding (re-enabled
 *     encoders re-encode the rows they don't already have rows for)
 *
 * Backend invariants enforced via `decide_enabled_write`:
 *   - at least one encoder must stay enabled (the IPC rejects an
 *     empty-list mutation with `BadInput`)
 *   - unknown encoder ids are rejected
 *   - the set is deduped + canonicalised (sorted) before persist so
 *     toggle-order doesn't churn settings.json
 *
 * Disabling an encoder does NOT delete its embeddings. They stay in
 * the per-encoder embeddings table; re-enabling brings them back
 * instantly with no re-encoding.
 */

interface EncoderInfo {
  id: string;
  display_name: string;
  description: string;
  dim: number;
  supports_text: boolean;
  supports_image: boolean;
}

export function EncoderSection() {
  const [encoders, setEncoders] = useState<EncoderInfo[] | null>(null);
  const [enabled, setEnabled] = useState<Set<string> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  // Initial load: list of available encoders + currently-enabled set.
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<EncoderInfo[]>("list_available_encoders"),
      invoke<string[]>("get_enabled_encoders"),
    ])
      .then(([list, enabledIds]) => {
        if (cancelled) return;
        setEncoders(list);
        setEnabled(new Set(enabledIds));
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return (
      <Section title="Encoders">
        <p className="text-xs text-destructive">{error}</p>
      </Section>
    );
  }
  if (!encoders || !enabled) {
    return (
      <Section title="Encoders">
        <p className="text-xs text-muted-foreground">Loading…</p>
      </Section>
    );
  }

  async function toggle(id: string, want: boolean) {
    if (!enabled) return;
    // Optimistic update + IPC. On error we reset state to whatever
    // the backend says it is (the backend is the source of truth).
    const next = new Set(enabled);
    if (want) {
      next.add(id);
    } else {
      next.delete(id);
    }
    if (next.size === 0) {
      // Frontend guard mirroring the backend's BadInput rejection.
      // The IPC would reject anyway; surfacing as a UI message saves
      // a roundtrip and a transient toggle bounce.
      setError("At least one encoder must stay enabled.");
      // Auto-clear after 3 s so the message doesn't stick.
      setTimeout(() => setError(null), 3000);
      return;
    }
    setEnabled(next);
    setPending(true);
    recordAction("encoder_toggle", { id, enabled: want });
    try {
      await invoke("set_enabled_encoders", { ids: Array.from(next) });
    } catch (e) {
      // Backend rejected — re-fetch authoritative state.
      console.warn("set_enabled_encoders failed:", e);
      try {
        const auth = await invoke<string[]>("get_enabled_encoders");
        setEnabled(new Set(auth));
      } catch {
        /* leave state as-is */
      }
      setError(String(e));
      setTimeout(() => setError(null), 3000);
    } finally {
      setPending(false);
    }
  }

  return (
    <Section title="Encoders">
      <p className="text-xs text-muted-foreground -mt-1">
        Enabled encoders run during indexing and contribute to{" "}
        <strong>multi-encoder fusion</strong> for image-image and
        text-image search. Disabling an encoder skips it on the next
        indexing pass and excludes it from fusion. Existing embeddings
        stay on disk — re-enabling is instant.
      </p>

      <div className="space-y-2">
        {encoders.map((enc) => (
          <EncoderToggle
            key={enc.id}
            info={enc}
            enabled={enabled.has(enc.id)}
            disabled={pending}
            onChange={(want) => toggle(enc.id, want)}
          />
        ))}
      </div>
    </Section>
  );
}

function EncoderToggle({
  info,
  enabled,
  disabled,
  onChange,
}: {
  info: EncoderInfo;
  enabled: boolean;
  disabled: boolean;
  onChange: (want: boolean) => void;
}) {
  return (
    <div className="rounded-md border border-border bg-secondary/40 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-xs font-medium text-foreground">
              {info.display_name}
            </span>
            <span className="text-[10px] text-muted-foreground">
              {info.dim}-dim
              {info.supports_image && info.supports_text
                ? " · image + text"
                : info.supports_image
                  ? " · image-only"
                  : " · text-only"}
            </span>
          </div>
          <details className="mt-1 text-[11px] text-muted-foreground">
            <summary className="cursor-pointer hover:text-foreground transition">
              What does this encoder bring?
            </summary>
            <p className="mt-1 pl-3 leading-relaxed">{info.description}</p>
          </details>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label={`Toggle ${info.display_name}`}
          disabled={disabled}
          onClick={() => onChange(!enabled)}
          className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-primary/50 disabled:opacity-50 disabled:cursor-not-allowed ${
            enabled ? "bg-primary" : "bg-input"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-background shadow-sm transition-transform ${
              enabled ? "translate-x-4" : "translate-x-0.5"
            }`}
          />
        </button>
      </div>
    </div>
  );
}
