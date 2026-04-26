import { describe, it, expect, vi, beforeEach } from "vitest";
import { StrictMode } from "react";
import { render, waitFor } from "@testing-library/react";

/**
 * Regression test for the duplicate-IPC bug surfaced in the on-exit
 * profiling report at t=5.87s — `set_priority_image_encoder` was firing
 * twice within ~13ms every time the settings drawer opened. Root cause
 * was React 19 StrictMode replaying mount effects in dev combined with
 * the SettingsDrawer being conditionally mounted (every drawer open
 * re-mounts EncoderSection, so the priority-push effect re-fired).
 *
 * The fix is a `useRef`-backed guard inside EncoderSection that skips
 * the IPC if the value hasn't changed since the last successful push.
 * These tests pin that behaviour:
 *   1. Under StrictMode + same value, the priority-push IPC fires once.
 *   2. When the value actually changes, the IPC fires again with the
 *      new id (no false-positive dedup).
 */

// Mock the Tauri IPC layer so we can count `invoke` calls per command.
// `list_available_encoders` is also called from this section's mount
// effect, so the mock has to satisfy that path as well.
const invokeMock = vi.fn(async (cmd: string, _args?: unknown) => {
  if (cmd === "list_available_encoders") {
    return [
      {
        id: "dinov2_base",
        display_name: "DINOv2 Base",
        description: "image-only encoder",
        dim: 768,
        supports_text: false,
        supports_image: true,
      },
      {
        id: "clip_vit_b_32",
        display_name: "CLIP ViT-B/32",
        description: "text + image",
        dim: 512,
        supports_text: true,
        supports_image: true,
      },
    ];
  }
  if (cmd === "set_priority_image_encoder") {
    return null;
  }
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

// Stub the perf service — the picker imports it for the breadcrumb
// recordAction on encoder change. Tests don't exercise that path, but
// the import has to resolve.
vi.mock("../../services/perf", () => ({
  recordAction: vi.fn(),
}));

import { EncoderSection, __resetEncoderPushCache } from "./EncoderSection";

beforeEach(() => {
  invokeMock.mockClear();
  // Default prefs land on dinov2_base — see useUserPreferences DEFAULTS.
  // We rely on that as the baseline for the dedup test.
  localStorage.clear();
  // The dedup cache is module-level (survives component unmounts on
  // purpose — that's exactly what makes it work under React 19
  // StrictMode + the SettingsDrawer's conditional mount). Tests need
  // to clear it between cases or the first test's pushed value
  // poisons the next.
  __resetEncoderPushCache();
});

describe("EncoderSection — IPC dedup (regression for double set_priority_image_encoder)", () => {
  it("fires set_priority_image_encoder ONCE under React 19 StrictMode", async () => {
    render(
      <StrictMode>
        <EncoderSection />
      </StrictMode>,
    );

    // StrictMode mounts → unmounts → re-mounts in dev. Without the
    // useRef guard, the priority-push effect would fire twice. With
    // the guard, the second mount sees `lastPushedRef === target` and
    // short-circuits.
    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(
        ([cmd]) => cmd === "set_priority_image_encoder",
      );
      expect(calls.length).toBe(1);
    });
  });

  it("re-pushes when the user actually changes the encoder (no false dedup)", async () => {
    // First mount with the default (dinov2_base from PREF_DEFAULTS).
    const { unmount } = render(<EncoderSection />);

    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(
        ([cmd]) => cmd === "set_priority_image_encoder",
      );
      expect(calls.length).toBe(1);
      expect(calls[0][1]).toEqual({ id: "dinov2_base" });
    });

    // Tear down the first mount cleanly — the module-level dedup cache
    // intentionally survives this unmount (that's the StrictMode-safety
    // property), so a genuine remount with the SAME value would NOT
    // re-fire. We're testing the OTHER branch: a remount with a
    // DIFFERENT value still pushes.
    unmount();

    // Simulate the user picking CLIP — write to localStorage (the same
    // shape useUserPreferences stores), then mount a fresh section so
    // the hook re-reads. A real user interaction goes via update();
    // the dedup behaviour we care about here is "the cache does not
    // block a genuine change" and the cleanest way to trigger that
    // path without coupling to the picker's <select> internals is via
    // storage + remount.
    localStorage.setItem(
      "imageBrowserPrefs",
      JSON.stringify({ imageEncoder: "clip_vit_b_32" }),
    );
    render(<EncoderSection />);

    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(
        ([cmd]) => cmd === "set_priority_image_encoder",
      );
      // First push (dinov2_base from initial mount) + second push
      // (clip_vit_b_32 after the genuine change). The dedup cache
      // remembers dinov2_base; a different value defeats the
      // short-circuit, which is exactly the safety property we want.
      expect(calls.length).toBeGreaterThanOrEqual(2);
      expect(calls[calls.length - 1][1]).toEqual({ id: "clip_vit_b_32" });
    });
  });

  it("does NOT re-push when the SettingsDrawer is reopened with the same value (regression for the duplicate at t=5.87s)", async () => {
    // First mount = first drawer open. Pushes dinov2_base.
    const first = render(<EncoderSection />);
    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(
        ([cmd]) => cmd === "set_priority_image_encoder",
      );
      expect(calls.length).toBe(1);
    });
    first.unmount();

    // Second mount = user closed the drawer and reopened it without
    // changing the encoder. Without the module-level dedup cache, this
    // would fire a second IPC — exactly the t=5.87s pattern. With it,
    // the new mount sees the cache holds the same value and skips.
    render(<EncoderSection />);

    // Give the effect a tick to run if it were going to.
    await new Promise((r) => setTimeout(r, 20));

    const calls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "set_priority_image_encoder",
    );
    expect(calls.length).toBe(1);
  });
});
