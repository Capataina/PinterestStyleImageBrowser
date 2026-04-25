import { describe, it, expect, vi, beforeEach } from "vitest";

/**
 * Tests for the IPC service layer.
 *
 * The real `invoke` function calls into the Rust backend. Under
 * vitest there's no Rust backend running, so we mock
 * @tauri-apps/api/core wholesale and inject the desired return value
 * (or rejection) per test. This catches regressions in:
 *   - argument shape passed to invoke
 *   - response handling and shape mapping
 *   - error propagation
 */

const mockInvoke = vi.fn();
const mockOpen = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
  convertFileSrc: (path: string) => `tauri://localhost/${path}`,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mockOpen,
}));

beforeEach(() => {
  mockInvoke.mockReset();
  mockOpen.mockReset();
});

describe("services/roots", () => {
  it("listRoots passes through the backend response", async () => {
    const { listRoots } = await import("./roots");
    mockInvoke.mockResolvedValueOnce([
      { id: 1, path: "/a", enabled: true, added_at: 100 },
      { id: 2, path: "/b", enabled: false, added_at: 200 },
    ]);
    const roots = await listRoots();
    expect(mockInvoke).toHaveBeenCalledWith("list_roots");
    expect(roots).toHaveLength(2);
    expect(roots[0].path).toBe("/a");
  });

  it("addRoot sends the path argument and returns the new root", async () => {
    const { addRoot } = await import("./roots");
    mockInvoke.mockResolvedValueOnce({
      id: 3,
      path: "/new",
      enabled: true,
      added_at: 300,
    });
    const root = await addRoot("/new");
    expect(mockInvoke).toHaveBeenCalledWith("add_root", { path: "/new" });
    expect(root.id).toBe(3);
  });

  it("removeRoot sends only the id argument", async () => {
    const { removeRoot } = await import("./roots");
    mockInvoke.mockResolvedValueOnce(undefined);
    await removeRoot(7);
    expect(mockInvoke).toHaveBeenCalledWith("remove_root", { id: 7 });
  });

  it("setRootEnabled sends both id and enabled", async () => {
    const { setRootEnabled } = await import("./roots");
    mockInvoke.mockResolvedValueOnce(undefined);
    await setRootEnabled(5, false);
    expect(mockInvoke).toHaveBeenCalledWith("set_root_enabled", {
      id: 5,
      enabled: false,
    });
  });

  it("listRoots wraps backend errors in an Error with context", async () => {
    const { listRoots } = await import("./roots");
    mockInvoke.mockRejectedValueOnce("backend exploded");
    await expect(listRoots()).rejects.toThrow(/Failed to list roots/);
  });
});

describe("services/notes", () => {
  it("getImageNotes passes the imageId arg and returns the string", async () => {
    const { getImageNotes } = await import("./notes");
    mockInvoke.mockResolvedValueOnce("a personal note");
    const notes = await getImageNotes(42);
    expect(mockInvoke).toHaveBeenCalledWith("get_image_notes", {
      imageId: 42,
    });
    expect(notes).toBe("a personal note");
  });

  it("setImageNotes sends both imageId and notes", async () => {
    const { setImageNotes } = await import("./notes");
    mockInvoke.mockResolvedValueOnce(undefined);
    await setImageNotes(42, "updated");
    expect(mockInvoke).toHaveBeenCalledWith("set_image_notes", {
      imageId: 42,
      notes: "updated",
    });
  });

  it("setImageNotes wraps errors", async () => {
    const { setImageNotes } = await import("./notes");
    mockInvoke.mockRejectedValueOnce("DB locked");
    await expect(setImageNotes(1, "x")).rejects.toThrow(/Failed to save notes/);
  });
});

describe("services/images", () => {
  it("fetchImages threads matchAllTags through to invoke", async () => {
    const { fetchImages } = await import("./images");
    mockInvoke.mockResolvedValueOnce([]);
    await fetchImages([1, 2], "skipped", true);
    expect(mockInvoke).toHaveBeenCalledWith("get_images", {
      filterTagIds: [1, 2],
      filterString: "skipped",
      matchAllTags: true,
    });
  });

  it("fetchImages defaults matchAllTags to false (OR semantic)", async () => {
    const { fetchImages } = await import("./images");
    mockInvoke.mockResolvedValueOnce([]);
    await fetchImages([1]);
    expect(mockInvoke).toHaveBeenCalledWith("get_images", {
      filterTagIds: [1],
      filterString: "",
      matchAllTags: false,
    });
  });

  it("fetchImages constructs convertFileSrc URLs for thumbnail and full", async () => {
    const { fetchImages } = await import("./images");
    mockInvoke.mockResolvedValueOnce([
      {
        id: 1,
        path: "/tmp/photo.jpg",
        name: "photo.jpg",
        thumbnail_path: "/tmp/thumb.jpg",
        width: 800,
        height: 600,
        tags: [],
      },
    ]);
    const items = await fetchImages();
    expect(items).toHaveLength(1);
    expect(items[0].url).toContain("photo.jpg");
    expect(items[0].thumbnailUrl).toContain("thumb.jpg");
  });

  it("fetchImages falls back to full-image URL when no thumbnail_path", async () => {
    const { fetchImages } = await import("./images");
    mockInvoke.mockResolvedValueOnce([
      {
        id: 1,
        path: "/tmp/photo.jpg",
        name: "photo.jpg",
        width: 800,
        height: 600,
        tags: [],
      },
    ]);
    const items = await fetchImages();
    expect(items[0].thumbnailUrl).toContain("photo.jpg");
  });

  it("setScanRoot sends only the path argument", async () => {
    const { setScanRoot } = await import("./images");
    mockInvoke.mockResolvedValueOnce(undefined);
    await setScanRoot("/new/folder");
    expect(mockInvoke).toHaveBeenCalledWith("set_scan_root", {
      path: "/new/folder",
    });
  });

  it("pickScanFolder returns the selected path", async () => {
    const { pickScanFolder } = await import("./images");
    mockOpen.mockResolvedValueOnce("/picked/path");
    const result = await pickScanFolder();
    expect(result).toBe("/picked/path");
    expect(mockOpen).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: "Choose your image folder",
    });
  });

  it("pickScanFolder returns null when user cancels", async () => {
    const { pickScanFolder } = await import("./images");
    mockOpen.mockResolvedValueOnce(null);
    const result = await pickScanFolder();
    expect(result).toBeNull();
  });

  it("semanticSearch invokes with query + topN", async () => {
    const { semanticSearch } = await import("./images");
    mockInvoke.mockResolvedValueOnce([]);
    await semanticSearch("cyberpunk", 25);
    expect(mockInvoke).toHaveBeenCalledWith("semantic_search", {
      query: "cyberpunk",
      topN: 25,
    });
  });
});

describe("services/tags", () => {
  it("createTag uses default colour when none provided", async () => {
    const { createTag } = await import("./tags");
    mockInvoke.mockResolvedValueOnce({
      id: 1,
      name: "test",
      color: "#3B82F6",
    });
    await createTag("test");
    expect(mockInvoke).toHaveBeenCalledWith("create_tag", {
      name: "test",
      color: "#3B82F6",
    });
  });

  it("createTag respects a custom colour", async () => {
    const { createTag } = await import("./tags");
    mockInvoke.mockResolvedValueOnce({ id: 1, name: "x", color: "#ff0000" });
    await createTag("x", "#ff0000");
    expect(mockInvoke).toHaveBeenCalledWith("create_tag", {
      name: "x",
      color: "#ff0000",
    });
  });

  it("deleteTag passes only the id", async () => {
    const { deleteTag } = await import("./tags");
    mockInvoke.mockResolvedValueOnce(undefined);
    await deleteTag(99);
    expect(mockInvoke).toHaveBeenCalledWith("delete_tag", { tagId: 99 });
  });

  it("fetchTags returns the backend list", async () => {
    const { fetchTags } = await import("./tags");
    mockInvoke.mockResolvedValueOnce([
      { id: 1, name: "a", color: "#fff" },
      { id: 2, name: "b", color: "#000" },
    ]);
    const tags = await fetchTags();
    expect(tags).toHaveLength(2);
  });
});
