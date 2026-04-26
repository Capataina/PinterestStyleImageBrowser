import { useEffect, useState, useCallback } from "react";

/**
 * User-facing UI preferences that don't need to round-trip through
 * the Rust backend. Stored in localStorage so they survive across
 * app launches; mirrored as React state so components re-render
 * when one changes.
 *
 * The schema is deliberately kept loose with serde-friendly defaults
 * — when we add a new field in a future version, older saved JSON
 * just deserialises with the missing field set to its default.
 */

export type ThemeMode = "system" | "dark" | "light";
export type SortMode = "shuffle" | "name" | "added";
export type AnimationLevel = "off" | "subtle" | "standard";
export type TagFilterMode = "any" | "all";

export interface UserPreferences {
  /** "system" follows the OS, others force. Default "system". */
  theme: ThemeMode;
  /**
   * Column count for the masonry grid. 0 = "auto" (compute from
   * container width / minItemWidth). Otherwise 1..8.
   */
  columnCount: number;
  /** Min tile width in pixels when columnCount is auto. */
  tileMinWidth: number;
  /** Sort order for the catalog. */
  sortMode: SortMode;
  /** Tile aspect-preserving size scale, multiplier of the base. */
  tileScale: number;
  /** Animation magnitude. */
  animationLevel: AnimationLevel;
  /** Number of results returned for "More like this". */
  similarResultCount: number;
  /** Number of results returned for semantic search. */
  semanticResultCount: number;
  /** Whether multi-tag filter ANDs (all) or ORs (any). */
  tagFilterMode: TagFilterMode;
  /**
   * Encoder used for "View Similar" (image→image) queries.
   * Matches the encoder_id values returned by the backend
   * `list_available_encoders` command.
   * Default `dinov2_base` — DINOv2 dominates CLIP for image-image
   * similarity by 2-5× on benchmarks. (Was `dinov2_small` in older
   * builds; that ID is dead post pipeline-version 2 and would route
   * every "View Similar" click to an empty cosine cache.)
   */
  imageEncoder: string;
  /**
   * Encoder used for text→image semantic search.
   * Default `clip_vit_b_32` because the SigLIP-2 text encoder
   * dispatch isn't fully wired yet — picker accepts the choice
   * but only CLIP path is functional today. SigLIP-2 text
   * dispatch is the natural next iteration.
   */
  textEncoder: string;
}

const DEFAULTS: UserPreferences = {
  theme: "system",
  columnCount: 0,
  tileMinWidth: 236,
  // Default to stable order (oldest first). Previously "shuffle" was
  // the default but combined with progressive thumbnail loading it
  // caused the visible "entire app refreshes" behaviour — every
  // refetch reshuffled the grid. Users who actively want shuffle can
  // pick it in Settings.
  sortMode: "added",
  tileScale: 1.0,
  animationLevel: "standard",
  similarResultCount: 35,
  semanticResultCount: 50,
  tagFilterMode: "any",
  imageEncoder: "dinov2_base",
  textEncoder: "clip_vit_b_32",
};

const STORAGE_KEY = "imageBrowserPrefs";
const THEME_KEY = "theme";

function loadFromStorage(): UserPreferences {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULTS };
    const parsed = JSON.parse(raw) as Partial<UserPreferences>;
    // Merge with defaults so newly-added fields land at their defaults
    // rather than undefined.
    const merged = { ...DEFAULTS, ...parsed };
    // Migrate legacy encoder IDs that no longer exist in the backend.
    // Pre pipeline-version-2 saved `dinov2_small` (384-d Small) — that
    // ID was wiped by the migration and the active encoder is
    // `dinov2_base` (768-d). Without this remap, returning users keep
    // dispatching against an empty cosine cache.
    if (merged.imageEncoder === "dinov2_small") {
      merged.imageEncoder = "dinov2_base";
    }
    return merged;
  } catch {
    return { ...DEFAULTS };
  }
}

function saveToStorage(prefs: UserPreferences) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
    // Theme is mirrored to its own key so main.tsx can read it before
    // React mounts (avoids the FOUC of wrong-theme flash).
    localStorage.setItem(THEME_KEY, prefs.theme);
  } catch {
    // localStorage may be disabled in some WebView modes; the in-memory
    // state still works, just doesn't persist.
  }
}

/**
 * Apply the theme preference to <html>. Called whenever the theme
 * setting changes; the initial application happens in main.tsx
 * before React mounts (which avoids a flash of wrong colours).
 */
function applyThemeToDom(theme: ThemeMode) {
  const root = document.documentElement;
  let dark: boolean;
  switch (theme) {
    case "dark":
      dark = true;
      break;
    case "light":
      dark = false;
      break;
    case "system":
    default:
      dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      break;
  }
  if (dark) root.classList.add("dark");
  else root.classList.remove("dark");
}

/**
 * React hook for the preferences object. Returns the current values
 * plus a setter.
 *
 * Theme application is reactive: changing prefs.theme flips the
 * `.dark` class on <html> immediately, which Tailwind's dark variants
 * pick up and the whole UI re-themes without a re-render of any
 * component.
 *
 * Listens to OS-level color-scheme changes when theme is "system" so
 * macOS auto-dark-mode flips the app theme along with everything else.
 */
export function useUserPreferences() {
  const [prefs, setPrefs] = useState<UserPreferences>(loadFromStorage);

  // Apply theme on every prefs change.
  useEffect(() => {
    applyThemeToDom(prefs.theme);
  }, [prefs.theme]);

  // Listen to OS-level theme changes when in "system" mode.
  useEffect(() => {
    if (prefs.theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyThemeToDom("system");
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [prefs.theme]);

  const update = useCallback(
    <K extends keyof UserPreferences>(key: K, value: UserPreferences[K]) => {
      setPrefs((prev) => {
        const next = { ...prev, [key]: value };
        saveToStorage(next);
        return next;
      });
    },
    [],
  );

  const resetAll = useCallback(() => {
    saveToStorage(DEFAULTS);
    setPrefs({ ...DEFAULTS });
  }, []);

  return { prefs, update, resetAll };
}

export const PREF_DEFAULTS = DEFAULTS;
