import { describe, expect, it } from "vitest";
import {
  createHtmlView,
  prepareHtmlForPreview,
  resolveLocalPath,
} from "./html-view";

describe("resolveLocalPath", () => {
  it("resolves relative references against the document directory", () => {
    expect(resolveLocalPath("/home/user/docs", "img/photo.png")).toBe(
      "/home/user/docs/img/photo.png",
    );
    expect(resolveLocalPath("/home/user/docs", "../shared/style.css")).toBe(
      "/home/user/shared/style.css",
    );
    expect(resolveLocalPath("/home/user/docs", "./a/./b.html")).toBe(
      "/home/user/docs/a/b.html",
    );
  });

  it("ignores external URLs, anchors, queries and root-absolute paths", () => {
    expect(resolveLocalPath("/docs", "https://example.com/a.png")).toBeNull();
    expect(resolveLocalPath("/docs", "data:image/png;base64,xx")).toBeNull();
    expect(resolveLocalPath("/docs", "#section")).toBeNull();
    expect(resolveLocalPath("/docs", "/etc/passwd")).toBeNull();
    expect(resolveLocalPath("/docs", "img.png?v=2#frag")).toBe("/docs/img.png");
    expect(resolveLocalPath("/docs", "../../..")).toBeNull();
  });
});

describe("prepareHtmlForPreview", () => {
  const options = {
    baseDir: "/root/docs",
    toAssetUrl: (path: string) => `asset://local${path}`,
  };

  it("removes scripts, event handlers and javascript: URLs", async () => {
    const html = await prepareHtmlForPreview(
      `<html><body><script>alert(1)</script><p onclick="evil()">x</p><a href="javascript:evil()">y</a></body></html>`,
      options,
    );
    expect(html).not.toContain("<script");
    expect(html).not.toContain("onclick");
    expect(html).not.toContain("javascript:");
    expect(html).toContain("<p>x</p>");
  });

  it("rewrites local image sources to asset URLs", async () => {
    const html = await prepareHtmlForPreview(
      `<html><body><img src="img/a.png"><img src="https://example.com/b.png"></body></html>`,
      options,
    );
    expect(html).toContain('src="asset://local/root/docs/img/a.png"');
    expect(html).toContain('src="https://example.com/b.png"');
  });

  it("inlines local stylesheets through the injected fetcher", async () => {
    const html = await prepareHtmlForPreview(
      `<html><head><link rel="stylesheet" href="style.css"></head><body></body></html>`,
      {
        ...options,
        fetchText: async (url: string) => {
          expect(url).toBe("asset://local/root/docs/style.css");
          return "p { color: red; }";
        },
      },
    );
    expect(html).toContain("<style>p { color: red; }</style>");
    expect(html).not.toContain("<link");
  });
});

describe("createHtmlView", () => {
  it("creates a fully sandboxed iframe with the document as srcdoc", () => {
    const frame = createHtmlView("<p>hello</p>", "test.html");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.srcdoc).toBe("<p>hello</p>");
    expect(frame.className).toBe("html-preview-frame");
  });
});
