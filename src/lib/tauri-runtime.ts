import { invoke as invokeTauriCommand } from "@tauri-apps/api/core";

const browserRuntimeMessage =
  "ViewerAのネイティブ機能はデスクトップアプリ内でのみ利用できます。ターミナルで「npm run tauri dev」を実行してください。";

export function invoke<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (
    typeof window === "undefined" ||
    !("__TAURI_INTERNALS__" in window)
  ) {
    return Promise.reject(new Error(browserRuntimeMessage));
  }
  return invokeTauriCommand<T>(command, args);
}
