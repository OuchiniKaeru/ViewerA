// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { renderZipEntries, type ZipEntry } from "./zip-preview";

describe("renderZipEntries", () => {
  it("renders archive metadata as inert text", () => {
    const container = document.createElement("div");
    const entries: ZipEntry[] = [
      {
        name: "<script>alert(1)</script>.txt",
        compressedSize: 12,
        uncompressedSize: 20,
        isDir: false,
      },
    ];

    renderZipEntries(container, entries, (size) => `${size} B`);

    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script>.txt");
    expect(container.textContent).toContain("20 B");
    expect(container.querySelectorAll("a")).toHaveLength(0);
  });

  it("shows a clear message for an empty archive", () => {
    const container = document.createElement("div");
    renderZipEntries(container, [], (size) => `${size} B`);

    expect(container.textContent).toContain("ZIP内にファイルがありません");
  });
});
