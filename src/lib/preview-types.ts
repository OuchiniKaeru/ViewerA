export type PreviewType =
  | "pdf"
  | "image"
  | "svg"
  | "text"
  | "html"
  | "mhtml"
  | "json"
  | "yaml"
  | "markdown"
  | "csv"
  | "spreadsheet"
  | "docx"
  | "pptx"
  | "audio"
  | "video"
  | "zip"
  | "unsupported";

const PREVIEW_TYPE_BY_EXTENSION: Readonly<Record<string, PreviewType>> = {
  pdf: "pdf",
  png: "image",
  jpg: "image",
  jpeg: "image",
  gif: "image",
  webp: "image",
  bmp: "image",
  svg: "svg",
  txt: "text",
  log: "text",
  py: "text",
  js: "text",
  jsx: "text",
  ts: "text",
  tsx: "text",
  html: "html",
  htm: "html",
  mhtml: "mhtml",
  mht: "mhtml",
  css: "text",
  c: "text",
  cc: "text",
  cpp: "text",
  cs: "text",
  h: "text",
  hpp: "text",
  java: "text",
  go: "text",
  kt: "text",
  kts: "text",
  php: "text",
  rb: "text",
  rs: "text",
  scala: "text",
  sh: "text",
  bash: "text",
  zsh: "text",
  sql: "text",
  swift: "text",
  toml: "text",
  xml: "text",
  ini: "text",
  cfg: "text",
  conf: "text",
  env: "text",
  properties: "text",
  plist: "text",
  json: "json",
  jsonc: "json",
  yaml: "yaml",
  yml: "yaml",
  md: "markdown",
  markdown: "markdown",
  csv: "csv",
  xlsx: "spreadsheet",
  xls: "spreadsheet",
  xlsb: "spreadsheet",
  ods: "spreadsheet",
  docx: "docx",
  pptx: "pptx",
  mp3: "audio",
  wav: "audio",
  ogg: "audio",
  oga: "audio",
  flac: "audio",
  m4a: "audio",
  aac: "audio",
  mp4: "video",
  m4v: "video",
  webm: "video",
  ogv: "video",
  mov: "video",
  mkv: "video",
  zip: "zip",
};

export function previewTypeForPath(path: string): PreviewType {
  const filename = path.split(/[\\/]/).at(-1) ?? "";
  const extension = filename.split(".").at(-1)?.toLowerCase();
  if (!extension || extension === filename.toLowerCase()) {
    return "unsupported";
  }
  return PREVIEW_TYPE_BY_EXTENSION[extension] ?? "unsupported";
}
