import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { IndexingProgress } from "../hooks/useIndexingProgress";

/**
 * Tests for the IndexingStatusPill component.
 *
 * The pill listens to the `useIndexingProgress` hook for state, then
 * decides whether to render and how. We mock the hook so each test
 * can inject any IndexingProgress shape we want and exercise the
 * rendering / visibility rules without firing real Tauri events.
 */

// Mutable mock value the mocked hook will return.
let mockProgressState: IndexingProgress | null = null;

vi.mock("../hooks/useIndexingProgress", async () => {
  // We import the type by re-exporting; runtime impl is replaced by
  // the mock below.
  return {
    useIndexingProgress: () => ({
      progress: mockProgressState,
      isIndexing:
        mockProgressState !== null &&
        mockProgressState.phase !== "ready" &&
        mockProgressState.phase !== "error",
    }),
  };
});

import { IndexingStatusPill } from "./IndexingStatusPill";

function renderPill() {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <IndexingStatusPill />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  mockProgressState = null;
});

describe("IndexingStatusPill", () => {
  it("renders nothing when no progress event has arrived", () => {
    mockProgressState = null;
    renderPill();
    expect(screen.queryByText(/Scanning/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Downloading/i)).not.toBeInTheDocument();
  });

  it("renders 'Scanning' label during the scan phase", () => {
    mockProgressState = {
      phase: "scan",
      processed: 47,
      total: 1500,
      message: null,
    };
    renderPill();
    expect(screen.getByText(/Scanning/i)).toBeInTheDocument();
  });

  it("renders 'Downloading models' label during model download", () => {
    mockProgressState = {
      phase: "model-download",
      processed: 100_000_000,
      total: 1_200_000_000,
      message: "Downloading model_image.onnx — 100 / 1200 MB",
    };
    renderPill();
    expect(screen.getByText(/Downloading models/i)).toBeInTheDocument();
  });

  it("formats counter as MB during model-download phase", () => {
    mockProgressState = {
      phase: "model-download",
      processed: 100 * 1_048_576,
      total: 1200 * 1_048_576,
      message: null,
    };
    renderPill();
    // 100 / 1200 MB
    expect(screen.getByText(/100 \/ 1200 MB/)).toBeInTheDocument();
  });

  it("formats counter as plain count for non-download phases", () => {
    mockProgressState = {
      phase: "thumbnail",
      processed: 247,
      total: 1500,
      message: null,
    };
    renderPill();
    expect(screen.getByText("247 / 1500")).toBeInTheDocument();
  });

  it("renders 'Ready' label with completion message when done", () => {
    mockProgressState = {
      phase: "ready",
      processed: 1842,
      total: 1842,
      message: "1842 images indexed",
    };
    renderPill();
    expect(screen.getByText(/Ready/i)).toBeInTheDocument();
    expect(screen.getByText("1842 images indexed")).toBeInTheDocument();
  });

  it("renders 'Error' label when phase is error", () => {
    mockProgressState = {
      phase: "error",
      processed: 0,
      total: 0,
      message: "Indexing failed: model not found",
    };
    renderPill();
    expect(screen.getByText(/Error/i)).toBeInTheDocument();
  });

  it("does not render the counter when total is 0 (indeterminate)", () => {
    mockProgressState = {
      phase: "scan",
      processed: 0,
      total: 0,
      message: "Starting...",
    };
    renderPill();
    // No "0 / 0" should appear.
    expect(screen.queryByText("0 / 0")).not.toBeInTheDocument();
  });

  it("uses progress.message as the title attribute when set", () => {
    mockProgressState = {
      phase: "scan",
      processed: 50,
      total: 100,
      message: "Scanning /Users/me/photos",
    };
    const { container } = renderPill();
    const pill = container.querySelector("[title]");
    expect(pill).toHaveAttribute("title", "Scanning /Users/me/photos");
  });
});
