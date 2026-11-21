export type MediaPreviewKind = "audio" | "video";

export function createMediaPreview(
  kind: MediaPreviewKind,
  source: string,
  label: string,
): HTMLMediaElement {
  const media =
    kind === "audio"
      ? document.createElement("audio")
      : document.createElement("video");
  media.className = `media-preview ${kind}-preview`;
  media.controls = true;
  media.autoplay = false;
  media.preload = "metadata";
  media.src = source;
  media.setAttribute("aria-label", label);
  if (kind === "video") {
    (media as HTMLVideoElement).playsInline = true;
  }
  return media;
}
