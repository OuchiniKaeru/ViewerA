const EXTERNAL_REFERENCE = /^(?:[a-z][a-z0-9+.-]*:|\/\/|#)/i;

/**
 * Resolve a relative reference inside an HTML document against the directory
 * that contains the file on disk. Returns null for external URLs, anchors and
 * absolute paths that cannot be mapped into the registered root.
 */
export function resolveLocalPath(
  baseDir: string,
  reference: string,
): string | null {
  const trimmed = reference.trim();
  if (!trimmed || EXTERNAL_REFERENCE.test(trimmed)) return null;
  const withoutQuery = trimmed.split(/[?#]/)[0];
  if (!withoutQuery || withoutQuery.startsWith("/")) return null;
  const segments = baseDir.split("/").filter(Boolean);
  for (const segment of withoutQuery.split("/")) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") {
      if (segments.length === 0) return null;
      segments.pop();
    } else {
      segments.push(segment);
    }
  }
  if (segments.length === 0) return null;
  return `/${segments.join("/")}`;
}

export type HtmlViewOptions = {
  baseDir: string;
  toAssetUrl: (absolutePath: string) => string;
  fetchText?: (url: string) => Promise<string>;
};

/**
 * Prepare an HTML document for the sandboxed preview frame:
 * - strip scripts, event handlers and javascript: URLs (defense in depth;
 *   the iframe sandbox already blocks script execution)
 * - point local images at the asset protocol
 * - inline local stylesheets so they survive the frame CSP
 */
export async function prepareHtmlForPreview(
  html: string,
  options: HtmlViewOptions,
): Promise<string> {
  const document = new DOMParser().parseFromString(html, "text/html");
  for (const script of Array.from(document.querySelectorAll("script"))) {
    script.remove();
  }
  for (const meta of Array.from(
    document.querySelectorAll('meta[http-equiv="refresh" i]'),
  )) {
    meta.remove();
  }
  for (const element of Array.from(document.querySelectorAll("*"))) {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (name.startsWith("on")) {
        element.removeAttribute(attribute.name);
      } else if (
        (name === "href" || name === "src" || name === "xlink:href") &&
        /^\s*javascript:/i.test(attribute.value)
      ) {
        element.removeAttribute(attribute.name);
      }
    }
  }

  for (const element of Array.from(
    document.querySelectorAll("img[src], source[src], video[src], audio[src], embed[src], input[type='image'][src]"),
  )) {
    const resolved = resolveLocalPath(options.baseDir, element.getAttribute("src") ?? "");
    if (resolved) element.setAttribute("src", options.toAssetUrl(resolved));
  }

  for (const link of Array.from(
    document.querySelectorAll("link[rel~='stylesheet'][href]"),
  )) {
    const resolved = resolveLocalPath(options.baseDir, link.getAttribute("href") ?? "");
    if (!resolved || !options.fetchText) {
      link.remove();
      continue;
    }
    try {
      const css = await options.fetchText(options.toAssetUrl(resolved));
      const style = document.createElement("style");
      style.textContent = css;
      link.replaceWith(style);
    } catch {
      link.remove();
    }
  }

  return `<!doctype html>\n${document.documentElement.outerHTML}`;
}

/**
 * Create the sandboxed HTML preview frame. The empty sandbox attribute
 * disables scripts, forms, popups and top-level navigation.
 */
export function createHtmlView(srcdoc: string, label: string): HTMLIFrameElement {
  const frame = document.createElement("iframe");
  frame.className = "html-preview-frame";
  frame.setAttribute("sandbox", "");
  frame.setAttribute("title", label);
  frame.srcdoc = srcdoc;
  return frame;
}
