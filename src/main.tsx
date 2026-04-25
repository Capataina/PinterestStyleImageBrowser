import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

/**
 * Apply the saved theme preference (or fall back to system preference)
 * before React mounts so the user never sees a flash of the wrong
 * theme. Settings drawer (Phase 9) writes to localStorage on toggle.
 *
 * Storage shape: { "theme": "dark" | "light" | "system" } — same JSON
 * the settings drawer reads/writes. Default is "system" so users on
 * macOS auto-dark-mode get dark, light-mode get light.
 */
function applyInitialTheme() {
  const root = document.documentElement;
  let saved: string | null = null;
  try {
    saved = localStorage.getItem("theme");
  } catch {
    // localStorage may be disabled in some WebView2 modes; fall back
    // to the system preference.
  }

  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const useDark =
    saved === "dark" || (saved !== "light" && (saved === "system" || saved === null) && prefersDark) ||
    // The "we have no saved value AND system has no preference" case
    // also defaults to dark — this app is image-focused, dark is the
    // visually-default for that use case.
    (saved === null && !prefersDark === false);

  if (useDark || saved === null) {
    root.classList.add("dark");
  }
}

applyInitialTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
