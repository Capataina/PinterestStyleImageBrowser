import { vi } from "vitest";

/**
 * Reusable Tauri IPC mock for service-layer tests.
 *
 * Usage from a test file:
 *
 *   import { mockInvoke } from "@/test/__mocks__/tauri";
 *   vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
 *
 * Then in individual tests:
 *
 *   mockInvoke.mockResolvedValueOnce([{ id: 1, path: "/x" }]);
 *
 * The mock is reset between tests automatically because of vitest's
 * `clearMocks: true` (configured in vitest.config.ts via the
 * default behaviour).
 */
export const mockInvoke = vi.fn();
