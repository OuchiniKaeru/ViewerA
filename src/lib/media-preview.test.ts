// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { createMediaPreview } from "./media-preview";

describe("createMediaPreview", () => {
  it("creates a user-controlled local audio player without autoplay", () => {
    const audio = createMediaPreview("audio", "asset://local/audio.mp3", "audio.mp3");

    expect(audio.tagName).toBe("AUDIO");
    expect(audio.controls).toBe(true);
    expect(audio.autoplay).toBe(false);
    expect(audio.preload).toBe("metadata");
    expect(audio.src).toContain("audio.mp3");
  });

  it("creates an inline video player without autoplay", () => {
    const video = createMediaPreview("video", "asset://local/video.mp4", "video.mp4");

    expect(video.tagName).toBe("VIDEO");
    expect(video.controls).toBe(true);
    expect(video.autoplay).toBe(false);
    expect(video.getAttribute("playsinline")).not.toBeNull();
    expect(video.preload).toBe("metadata");
  });
});
