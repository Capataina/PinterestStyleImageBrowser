import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "path";

/**
 * Vitest config for the frontend test suite.
 *
 * - happy-dom is the lightest jsdom alternative; React 19 + RTL works
 *   on it out of the box and it's faster than jsdom for our needs.
 * - The same `@/` alias the app uses (mapped to /src) is replicated
 *   here so test imports look the same as production code.
 * - setupFiles wires up jest-dom matchers (`toBeInTheDocument`, etc.)
 *   and the per-test localStorage / DOM cleanup.
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "happy-dom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    // Exclude node_modules + the Rust src-tauri dir from any glob
    // walks — vitest's defaults already cover node_modules but
    // src-tauri occasionally shows up in test glob plugins.
    exclude: ["node_modules", "src-tauri", "dist", "Library"],
    // Coverage targets every src/ TS module by default. Components
    // and hooks are the priority; services and types are simpler.
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/test/**",
        "src/**/*.d.ts",
        "src/main.tsx",
        "src/vite-env.d.ts",
      ],
    },
  },
});
