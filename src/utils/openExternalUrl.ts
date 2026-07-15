import { transport } from "../services/transport";

function isSafeExternalUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" || parsed.protocol === "http:";
  } catch {
    return false;
  }
}

export function openExternalUrl(url: string): void {
  if (!isSafeExternalUrl(url)) {
    console.warn("[openExternalUrl] blocked unsupported URL:", url);
    return;
  }

  if (transport.isLocal()) {
    void import("@tauri-apps/plugin-shell")
      .then(({ open }) => open(url))
      .catch((error) => {
        console.warn("[openExternalUrl] failed to open URL:", error);
      });
    return;
  }

  const opened = window.open(url, "_blank", "noopener,noreferrer");
  if (opened) opened.opener = null;
}
