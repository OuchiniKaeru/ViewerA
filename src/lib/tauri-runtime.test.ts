import { describe, expect, it } from "vitest";
import { invoke } from "./tauri-runtime";

describe("Tauri runtime access", () => {
  it("explains that native commands require the desktop application", async () => {
    await expect(invoke("select_root_folder")).rejects.toThrow(
      "ViewerAのネイティブ機能はデスクトップアプリ内でのみ利用できます。ターミナルで「npm run tauri dev」を実行してください。",
    );
  });

  it("forwards commands to the Tauri IPC bridge when available", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: (command: string, args: Record<string, unknown>) =>
          Promise.resolve({ command, args }),
      },
    });

    try {
      await expect(invoke("select_root_folder", { source: "toolbar" })).resolves.toEqual({
        command: "select_root_folder",
        args: { source: "toolbar" },
      });
    } finally {
      Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    }
  });
});
