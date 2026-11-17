import { sha256 } from "hash-wasm";

// getRandomValues is available on LAN HTTP pages, unlike randomUUID/subtle.
export function taskId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  const hex = Array.from(bytes, (v) => v.toString(16).padStart(2, "0")).join(
    "",
  );
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
export async function chunkDigest(blob: Blob): Promise<string> {
  const data = await blob.arrayBuffer();
  if (crypto.subtle) {
    return Array.from(
      new Uint8Array(await crypto.subtle.digest("SHA-256", data)),
      (v) => v.toString(16).padStart(2, "0"),
    ).join("");
  }
  return sha256(new Uint8Array(data));
}
export async function copyText(value: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value);
      return;
    } catch {
      /* Use the HTTP-compatible selection fallback. */
    }
  }
  const focus =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  const input = document.createElement("textarea");
  input.value = value;
  input.readOnly = true;
  input.style.cssText =
    "position:fixed;opacity:0;top:0;left:0;pointer-events:none";
  // Keep the temporary selection inside an open modal, if present.
  (document.querySelector("dialog[open]") || document.body).append(input);
  try {
    input.focus();
    input.select();
    if (!document.execCommand("copy")) throw new Error("copy_failed");
  } finally {
    input.remove();
    focus?.focus();
  }
}
