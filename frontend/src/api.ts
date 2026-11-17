export interface Node {
  id: string;
  parent?: string;
  name: string;
  path?: string;
  kind: "file" | "folder";
  size: number;
  mtime: number;
  deleted?: boolean;
}
export interface Upload {
  id: string;
  parent: string;
  name: string;
  size: number;
  digest: string;
  strategy: string;
  expires: number;
  state: string;
  node?: string;
}
export interface UploadStatus {
  upload: Upload;
  chunk_size: number;
  chunks: { index: number; digest: string; size: number }[];
}
let csrf = "";
export function setCsrf(value: string) {
  csrf = value;
}
export class ApiError extends Error {
  constructor(
    public code: string,
    public status: number,
    public details: unknown,
  ) {
    super(code);
  }
}
async function fetchResponse(
  path: string,
  options: RequestInit,
): Promise<Response> {
  try {
    return await fetch(path, options);
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError") throw error;
    throw new ApiError("network_error", 0, null);
  }
}
export async function api<T = any>(
  path: string,
  method = "GET",
  data?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const res = await fetchResponse(`/api${path}`, {
    method,
    credentials: "same-origin",
    headers: {
      ...(data !== undefined ? { "Content-Type": "application/json" } : {}),
      ...(method !== "GET" ? { "X-CSRF-Token": csrf } : {}),
    },
    body: data !== undefined ? JSON.stringify(data) : undefined,
    signal,
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({
      error: {
        code:
          res.status === 502 || res.status === 503
            ? "service_unavailable"
            : "request_failed",
      },
    }));
    throw new ApiError(
      e.error?.code || "request_failed",
      res.status,
      e.error?.details,
    );
  }
  return res.json();
}
export async function putChunk(
  id: string,
  index: number,
  blob: Blob,
  digest: string,
  signal: AbortSignal,
) {
  const res = await fetchResponse(`/api/uploads/${id}/chunks/${index}`, {
    method: "PUT",
    credentials: "same-origin",
    headers: { "X-CSRF-Token": csrf, "X-Chunk-SHA256": digest },
    body: blob,
    signal,
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new ApiError(
      e.error?.code || "chunk_failed",
      res.status,
      e.error?.details,
    );
  }
  return res.json();
}
export const bytes = (n: number) =>
  n < 1000
    ? `${n} B`
    : new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(
        n / 1000 ** Math.min(3, Math.floor(Math.log10(n) / 3)),
      ) +
      " " +
      ["B", "KB", "MB", "GB"][Math.min(3, Math.floor(Math.log10(n) / 3))];
export const date = (n: number) => new Date(n * 1000).toLocaleString();
