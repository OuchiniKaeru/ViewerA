export type MhtmlPart = {
  location: string;
  contentId: string;
  contentType: string;
  charset: string;
  bytes: Uint8Array;
};

export type MhtmlDocument = {
  html: string;
  mainLocation: string;
  parts: MhtmlPart[];
};

function binaryString(bytes: Uint8Array): string {
  let result = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    result += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return result;
}

function bytesFromBinaryString(value: string): Uint8Array {
  const bytes = new Uint8Array(value.length);
  for (let index = 0; index < value.length; index += 1) {
    bytes[index] = value.charCodeAt(index) & 0xff;
  }
  return bytes;
}

function base64ToBytes(value: string): Uint8Array {
  const cleaned = value.replace(/\s+/g, "");
  return bytesFromBinaryString(atob(cleaned));
}

function bytesToBase64(bytes: Uint8Array): string {
  return btoa(binaryString(bytes));
}

function decodeQuotedPrintable(value: string): Uint8Array {
  const output: number[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (character !== "=") {
      output.push(value.charCodeAt(index) & 0xff);
      continue;
    }
    if (value[index + 1] === "\r" && value[index + 2] === "\n") {
      index += 2;
      continue;
    }
    if (value[index + 1] === "\n") {
      index += 1;
      continue;
    }
    const hex = value.slice(index + 1, index + 3);
    if (/^[0-9a-f]{2}$/i.test(hex)) {
      output.push(parseInt(hex, 16));
      index += 2;
    } else {
      output.push(0x3d);
    }
  }
  return new Uint8Array(output);
}

function parseHeaders(block: string): Map<string, string> {
  const headers = new Map<string, string>();
  const unfolded = block.replace(/\r?\n[ \t]+/g, " ");
  for (const line of unfolded.split(/\r?\n/)) {
    const separator = line.indexOf(":");
    if (separator <= 0) continue;
    const name = line.slice(0, separator).trim().toLowerCase();
    const value = line.slice(separator + 1).trim();
    if (!headers.has(name)) headers.set(name, value);
  }
  return headers;
}

function headerParameter(header: string, name: string): string | null {
  const pattern = new RegExp(`;\\s*${name}\\s*=\\s*(?:"([^"]*)"|([^;\\s]+))`, "i");
  const match = pattern.exec(header);
  if (!match) return null;
  return match[1] ?? match[2] ?? null;
}

function splitHeaderBody(part: string): { headers: string; body: string } {
  const doubleCrlf = part.indexOf("\r\n\r\n");
  const doubleLf = part.indexOf("\n\n");
  let separator = -1;
  let separatorLength = 0;
  if (doubleCrlf !== -1 && (doubleLf === -1 || doubleCrlf <= doubleLf)) {
    separator = doubleCrlf;
    separatorLength = 4;
  } else if (doubleLf !== -1) {
    separator = doubleLf;
    separatorLength = 2;
  }
  if (separator === -1) return { headers: part, body: "" };
  return {
    headers: part.slice(0, separator),
    body: part.slice(separator + separatorLength),
  };
}

/** Parse a MIME MHTML archive into its decoded parts. */
export function parseMhtml(data: ArrayBuffer): MhtmlDocument {
  const bytes = new Uint8Array(data);
  const text = binaryString(bytes);
  const { headers: topBlock, body } = splitHeaderBody(text);
  const topHeaders = parseHeaders(topBlock);
  const topType = topHeaders.get("content-type") ?? "";
  const boundary = headerParameter(topType, "boundary");
  if (!boundary) {
    throw new Error("MHTMLファイルにmultipart boundaryがありません。");
  }

  const delimiter = `--${boundary}`;
  const parts: MhtmlPart[] = [];
  let cursor = body.indexOf(delimiter);
  while (cursor !== -1) {
    const afterDelimiter = cursor + delimiter.length;
    if (body.slice(afterDelimiter, afterDelimiter + 2) === "--") break;
    let lineEnd = body.indexOf("\n", afterDelimiter);
    if (lineEnd === -1) break;
    lineEnd += 1;
    const next = body.indexOf(`\n${delimiter}`, lineEnd);
    const partEnd = next === -1 ? body.length : next + 1;
    let partText = body.slice(lineEnd, partEnd);
    partText = partText.replace(/\r?\n$/, "");
    const { headers: headerBlock, body: partBody } = splitHeaderBody(partText);
    const headers = parseHeaders(headerBlock);
    const contentTypeHeader = headers.get("content-type") ?? "application/octet-stream";
    const contentType = contentTypeHeader.split(";")[0].trim().toLowerCase();
    const charset = (headerParameter(contentTypeHeader, "charset") ?? "utf-8")
      .replace(/^"|"$/g, "")
      .toLowerCase();
    const encoding = (headers.get("content-transfer-encoding") ?? "8bit")
      .trim()
      .toLowerCase();
    let partBytes: Uint8Array;
    if (encoding === "base64") {
      partBytes = base64ToBytes(partBody);
    } else if (encoding === "quoted-printable") {
      partBytes = decodeQuotedPrintable(partBody);
    } else {
      partBytes = bytesFromBinaryString(partBody);
    }
    parts.push({
      location: headers.get("content-location") ?? "",
      contentId: (headers.get("content-id") ?? "").replace(/^<|>$/g, ""),
      contentType,
      charset,
      bytes: partBytes,
    });
    cursor = next === -1 ? -1 : next + 1;
  }

  if (parts.length === 0) {
    throw new Error("MHTMLファイルにパートがありません。");
  }

  const start = headerParameter(topType, "start")?.replace(/^<|>$/g, "");
  const snapshotLocation = topHeaders.get("snapshot-content-location");
  const main =
    (start && parts.find((part) => part.contentId === start)) ||
    (snapshotLocation &&
      parts.find((part) => part.location === snapshotLocation)) ||
    parts.find((part) => part.contentType === "text/html") ||
    parts[0];

  let html: string;
  try {
    html = new TextDecoder(main.charset || "utf-8").decode(main.bytes);
  } catch {
    html = new TextDecoder("utf-8").decode(main.bytes);
  }
  return { html, mainLocation: main.location, parts };
}

function resolveAgainstMain(reference: string, mainLocation: string): string {
  try {
    return new URL(reference, mainLocation || undefined).href;
  } catch {
    return reference;
  }
}

/**
 * Replace resource references in the main HTML part with data: URLs built
 * from the archive parts, so the document renders inside a sandboxed frame
 * without any network access.
 */
export function inlineMhtmlResources(archive: MhtmlDocument): string {
  const resources = new Map<string, string>();
  for (const part of archive.parts) {
    if (!part.location || part.contentType === "text/html") continue;
    resources.set(
      part.location,
      `data:${part.contentType};base64,${bytesToBase64(part.bytes)}`,
    );
  }
  const lookup = (reference: string): string | null => {
    if (!reference || reference.startsWith("data:")) return null;
    return (
      resources.get(reference) ??
      resources.get(resolveAgainstMain(reference, archive.mainLocation)) ??
      null
    );
  };
  const findPart = (reference: string): MhtmlPart | null =>
    archive.parts.find(
      (part) =>
        part.location === reference ||
        part.location === resolveAgainstMain(reference, archive.mainLocation),
    ) ?? null;

  const document = new DOMParser().parseFromString(archive.html, "text/html");
  for (const link of Array.from(
    document.querySelectorAll("link[rel~='stylesheet'][href]"),
  )) {
    const part = findPart(link.getAttribute("href") ?? "");
    if (!part || part.contentType !== "text/css") continue;
    let css: string;
    try {
      css = new TextDecoder(part.charset || "utf-8").decode(part.bytes);
    } catch {
      css = new TextDecoder("utf-8").decode(part.bytes);
    }
    const style = document.createElement("style");
    style.textContent = replaceCssUrls(css, lookup);
    link.replaceWith(style);
  }
  for (const element of Array.from(document.querySelectorAll("*"))) {
    for (const name of ["src", "href", "poster", "background"]) {
      const value = element.getAttribute(name);
      if (!value) continue;
      const replacement = lookup(value);
      if (replacement) element.setAttribute(name, replacement);
    }
    const style = element.getAttribute("style");
    if (style) {
      element.setAttribute("style", replaceCssUrls(style, lookup));
    }
  }
  for (const style of Array.from(document.querySelectorAll("style"))) {
    style.textContent = replaceCssUrls(style.textContent ?? "", lookup);
  }
  return `<!doctype html>\n${document.documentElement.outerHTML}`;
}

function replaceCssUrls(
  css: string,
  lookup: (reference: string) => string | null,
): string {
  return css.replace(
    /url\(\s*(['"]?)([^'")]+)\1\s*\)/gi,
    (match, _quote: string, reference: string) => {
      const replacement = lookup(reference.trim());
      return replacement ? `url("${replacement}")` : match;
    },
  );
}
