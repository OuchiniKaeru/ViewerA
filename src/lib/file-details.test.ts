// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { createFileDetailsDialog } from "./file-details";

describe("createFileDetailsDialog", () => {
  it("shows file metadata as text in an accessible dialog", () => {
    const dialog = createFileDetailsDialog({
      name: "<report>.pdf",
      size: "1.5 MB",
      type: "PDF",
      mimeType: "application/pdf",
      modifiedAt: "2026/11/16",
      relativePath: "reports/<report>.pdf",
      rootLabel: "Documents",
    });

    expect(dialog.getAttribute("aria-labelledby")).toBeTruthy();
    expect(dialog.querySelector("h2")?.textContent).toBe("ファイル詳細");
    expect(dialog.querySelector("dd")?.textContent).toBe("<report>.pdf");
    expect(dialog.querySelector("dd")?.querySelector("img")).toBeNull();
    expect(dialog.textContent).toContain("application/pdf");
    expect(dialog.textContent).toContain("reports/<report>.pdf");
  });

  it("closes and removes the dialog using its close control", () => {
    document.body.innerHTML = "";
    const dialog = createFileDetailsDialog({
      name: "notes.txt",
      size: "5 B",
      type: "TEXT",
      mimeType: "text/plain",
      modifiedAt: "—",
      relativePath: "notes.txt",
      rootLabel: "Documents",
    });
    document.body.append(dialog);
    dialog.setAttribute("open", "");
    dialog.querySelector<HTMLButtonElement>("[data-dialog-close]")?.click();

    expect(dialog.isConnected).toBe(false);
  });
});
