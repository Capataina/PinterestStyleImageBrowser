import { describe, it, expect, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useUserPreferences, PREF_DEFAULTS } from "./useUserPreferences";

/**
 * Tests for the useUserPreferences hook.
 *
 * The hook owns:
 *   - default preferences when localStorage is empty,
 *   - localStorage round-trip on update,
 *   - DOM class application for the theme,
 *   - system-theme-follow via matchMedia.
 */

describe("useUserPreferences", () => {
  it("returns default preferences when localStorage is empty", () => {
    const { result } = renderHook(() => useUserPreferences());
    expect(result.current.prefs).toEqual(PREF_DEFAULTS);
  });

  it("loads previously-saved preferences from localStorage", () => {
    localStorage.setItem(
      "imageBrowserPrefs",
      JSON.stringify({ ...PREF_DEFAULTS, columnCount: 5 }),
    );
    const { result } = renderHook(() => useUserPreferences());
    expect(result.current.prefs.columnCount).toBe(5);
  });

  it("merges saved values with defaults so newly-added fields land at default", () => {
    // Old saved JSON missing the `tagFilterMode` field that was
    // added later. The hook should fill it in from defaults rather
    // than leaving it undefined.
    localStorage.setItem(
      "imageBrowserPrefs",
      JSON.stringify({ columnCount: 4 }),
    );
    const { result } = renderHook(() => useUserPreferences());
    expect(result.current.prefs.columnCount).toBe(4);
    expect(result.current.prefs.tagFilterMode).toBe(PREF_DEFAULTS.tagFilterMode);
    expect(result.current.prefs.theme).toBe(PREF_DEFAULTS.theme);
  });

  it("falls back to defaults when localStorage JSON is corrupt", () => {
    localStorage.setItem("imageBrowserPrefs", "not-json-at-all");
    const { result } = renderHook(() => useUserPreferences());
    expect(result.current.prefs).toEqual(PREF_DEFAULTS);
  });

  it("persists updates to localStorage", () => {
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("columnCount", 7);
    });
    expect(result.current.prefs.columnCount).toBe(7);
    const saved = JSON.parse(
      localStorage.getItem("imageBrowserPrefs") as string,
    );
    expect(saved.columnCount).toBe(7);
  });

  it("mirrors theme to its own localStorage key for main.tsx pre-mount read", () => {
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("theme", "dark");
    });
    expect(localStorage.getItem("theme")).toBe("dark");
  });

  it("applies the .dark class on documentElement when theme is dark", () => {
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("theme", "dark");
    });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("removes the .dark class when theme is light", () => {
    document.documentElement.classList.add("dark");
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("theme", "light");
    });
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("respects system dark preference when theme is 'system'", () => {
    // Override the default matchMedia (which says no dark preference)
    // to claim dark IS preferred. The hook should apply .dark.
    (window.matchMedia as ReturnType<typeof vi.fn>).mockImplementation(
      (query: string) => ({
        matches: query === "(prefers-color-scheme: dark)",
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      }),
    );
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("theme", "system");
    });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("resetAll wipes localStorage and restores defaults", () => {
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("columnCount", 4);
      result.current.update("tileScale", 1.5);
    });
    act(() => {
      result.current.resetAll();
    });
    expect(result.current.prefs).toEqual(PREF_DEFAULTS);
  });

  it("preserves unrelated fields when updating one", () => {
    const { result } = renderHook(() => useUserPreferences());
    act(() => {
      result.current.update("columnCount", 4);
    });
    act(() => {
      result.current.update("tileScale", 1.5);
    });
    expect(result.current.prefs.columnCount).toBe(4);
    expect(result.current.prefs.tileScale).toBe(1.5);
  });
});
