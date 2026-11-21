// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { renderFileReferences, type FileReference } from "./file-references";

describe("renderFileReferences", () => {
  it("renders escaped labels and opens the selected reference", () => {
    const container = document.createElement("div");
    const onSelect = vi.fn();
    const reference: FileReference = {
      rootId: "root-1",
      relativePath: "reports/<latest>.txt",
      timestamp: "2026-11-16T00:00:00Z",
    };

    renderFileReferences(container, [reference], "一覧は空です", onSelect);
    const button = container.querySelector("button");
    expect(button?.textContent).toBe("<latest>.txt");
    expect(button?.querySelector("img")).toBeNull();
    expect(button?.title).toBe("reports/<latest>.txt");
    button?.click();
    expect(onSelect).toHaveBeenCalledWith(reference);
  });

  it("shows a clear empty state when no references exist", () => {
    const container = document.createElement("div");
    renderFileReferences(container, [], "お気に入りはありません", vi.fn());

    expect(container.textContent).toBe("お気に入りはありません");
    expect(container.querySelector("button")).toBeNull();
  });
});
