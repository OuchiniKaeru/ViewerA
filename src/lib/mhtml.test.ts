import { describe, expect, it } from "vitest";
import { inlineMhtmlResources, parseMhtml } from "./mhtml";

const PNG_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

const FIXTURE = [
  "From: <Saved by Test>",
  "Snapshot-Content-Location: https://example.com/page.html",
  "MIME-Version: 1.0",
  'Content-Type: multipart/related; type="text/html";',
  '\tboundary="BOUNDARY123"',
  "",
  "--BOUNDARY123",
  "Content-Type: text/html; charset=utf-8",
  "Content-ID: <frame-0@mhtml.blink>",
  "Content-Transfer-Encoding: quoted-printable",
  "Content-Location: https://example.com/page.html",
  "",
  '<html><head><link rel=3D"stylesheet" href=3D"style.css"></head><body><img =',
  'src=3D"img/logo.png"><p>=E3=81=82</p></body></html>',
  "--BOUNDARY123",
  "Content-Type: text/css",
  "Content-Transfer-Encoding: quoted-printable",
  "Content-Location: https://example.com/style.css",
  "",
  "p { color: red; background: url(img/logo.png); }",
  "--BOUNDARY123",
  "Content-Type: image/png",
  "Content-Transfer-Encoding: base64",
  "Content-Location: https://example.com/img/logo.png",
  "",
  PNG_BASE64,
  "--BOUNDARY123--",
  "",
].join("\r\n");

function fixtureBuffer(): ArrayBuffer {
  const bytes = new TextEncoder().encode(FIXTURE);
  return bytes.buffer as ArrayBuffer;
}

describe("parseMhtml", () => {
  it("splits parts, decodes transfer encodings and picks the main HTML part", () => {
    const archive = parseMhtml(fixtureBuffer());
    expect(archive.parts).toHaveLength(3);
    expect(archive.mainLocation).toBe("https://example.com/page.html");
    expect(archive.html).toContain('rel="stylesheet"');
    expect(archive.html).toContain("あ");
    const image = archive.parts.find((part) => part.contentType === "image/png");
    expect(image?.bytes.length).toBeGreaterThan(0);
    expect(image?.bytes[0]).toBe(0x89);
    expect(image?.bytes[1]).toBe(0x50);
  });

  it("rejects archives without a boundary", () => {
    const bytes = new TextEncoder().encode("Content-Type: text/html\r\n\r\n<p>x</p>");
    expect(() => parseMhtml(bytes.buffer as ArrayBuffer)).toThrow();
  });
});

describe("inlineMhtmlResources", () => {
  it("embeds images as data URLs and inlines stylesheets", () => {
    const html = inlineMhtmlResources(parseMhtml(fixtureBuffer()));
    expect(html).toContain(`src="data:image/png;base64,${PNG_BASE64}"`);
    expect(html).toContain("<style>");
    expect(html).toContain("color: red;");
    expect(html).toContain(`url("data:image/png;base64,${PNG_BASE64}")`);
    expect(html).not.toContain("<link");
    expect(html).toContain("あ");
  });
});
