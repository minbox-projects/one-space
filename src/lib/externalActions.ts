import { invoke } from "@tauri-apps/api/core";

function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export async function openExternalUrl(url: string) {
  if (isTauriRuntime()) {
    await invoke("open_external_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function openLocalPath(path: string) {
  if (!isTauriRuntime()) {
    throw new Error("Local path opening is only available in the desktop app.");
  }
  await invoke("open_local_path", { path });
}

export function isLikelyLocalPath(value: string) {
  return (
    value.startsWith("/") ||
    value.startsWith("~/") ||
    /^[A-Za-z]:\\/.test(value)
  );
}
