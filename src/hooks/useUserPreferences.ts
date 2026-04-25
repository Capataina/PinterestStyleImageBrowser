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
}

const DEFAULTS: UserPreferences = {
  theme: "system",
  columnCount: 0,
  tileMinWidth: 236,
  sortMode: "shuffle",
  tileScale: 1.0,
  animationLevel: "standard",
  similarResultCount: 35,
  semanticResultCount: 50,
  tagFilterMode: "any",
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
    return { ...DEFAULTS, ...parsed };
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
