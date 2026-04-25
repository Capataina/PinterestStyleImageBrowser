import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

/**
 * Global test setup. Runs once per test file before any tests, then
 * the per-test hooks reset state between tests.
 */

/**
 * In-memory localStorage shim. happy-dom 20.x doesn't expose all the
 * Storage methods we need (clear, setItem, getItem, removeItem) on
 * window.localStorage as expected. We replace it wholesale with a
 * plain Map-backed implementation that satisfies the Storage
 * interface for our purposes.
 *
 * Using `defineProperty` rather than direct assignment because
 * happy-dom defines `localStorage` as a non-writable accessor.
 */
class MockStorage implements Storage {
  private store = new Map<string, string>();

  get length(): number {
    return this.store.size;
  }

  clear(): void {
    this.store.clear();
  }

  getItem(key: string): string | null {
    return this.store.has(key) ? (this.store.get(key) as string) : null;
  }

  key(index: number): string | null {
    return Array.from(this.store.keys())[index] ?? null;
  }

  removeItem(key: string): void {
    this.store.delete(key);
  }

  setItem(key: string, value: string): void {
    this.store.set(key, String(value));
  }
}

const mockLocalStorage = new MockStorage();

Object.defineProperty(window, "localStorage", {
  configurable: true,
  writable: true,
  value: mockLocalStorage,
});

Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  writable: true,
  value: mockLocalStorage,
});

// Reset DOM and react roots after every test.
afterEach(() => {
  cleanup();
});

// Clear our mock storage before every test so the useUserPreferences
// hook starts from defaults each time.
beforeEach(() => {
  mockLocalStorage.clear();
  // Reset the html.dark class — the prefs hook applies it on mount,
  // so a previous test's theme leak would otherwise affect later tests.
  document.documentElement.classList.remove("dark");
});

/**
 * matchMedia mock. happy-dom provides one but it returns false for
 * everything; tests that exercise system-theme-follow logic need to
 * inject their own answer. We default to "no dark preference" so
 * useUserPreferences's system-mode falls through to light unless the
 * test overrides.
 */
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});
