import { describe, expect, it } from "vitest";
import { previewTypeForPath } from "./preview-types";

describe("previewTypeForPath", () => {
  it("recognizes supported preview types without relying on filename case", () => {
    expect(previewTypeForPath("REPORT.PDF")).toBe("pdf");
    expect(previewTypeForPath("photo.WebP")).toBe("image");
    expect(previewTypeForPath("notes.Markdown")).toBe("markdown");
    expect(previewTypeForPath("book.XLSX")).toBe("spreadsheet");
  });

  it("does not mistake legacy binary Office formats for OOXML", () => {
    expect(previewTypeForPath("legacy.doc")).toBe("unsupported");
    expect(previewTypeForPath("legacy.ppt")).toBe("unsupported");
    expect(previewTypeForPath("legacy.xls")).toBe("spreadsheet");
  });

  it("supports common source and config text formats", () => {
    expect(previewTypeForPath("script.sh")).toBe("text");
    expect(previewTypeForPath("query.SQL")).toBe("text");
    expect(previewTypeForPath("app.env")).toBe("text");
    expect(previewTypeForPath("settings.JSONC")).toBe("json");
  });

  it("recognizes audio, video, and ZIP preview types", () => {
    expect(previewTypeForPath("audio.MP3")).toBe("audio");
    expect(previewTypeForPath("video.WebM")).toBe("video");
    expect(previewTypeForPath("archive.ZIP")).toBe("zip");
  });

  it("renders HTML and MHTML documents in the HTML view", () => {
    expect(previewTypeForPath("page.HTML")).toBe("html");
    expect(previewTypeForPath("page.htm")).toBe("html");
    expect(previewTypeForPath("snapshot.MHTML")).toBe("mhtml");
    expect(previewTypeForPath("snapshot.mht")).toBe("mhtml");
  });
});
