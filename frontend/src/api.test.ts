import { afterEach, describe, expect, it, vi } from "vitest";
import { api, putChunk } from "./api";
afterEach(() => vi.unstubAllGlobals());
describe("user-facing connection failures", () => {
  it("reports a consistent network error for API and chunk failures", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockRejectedValue(new TypeError("Failed to fetch")),
    );
    await expect(api("/auth/session")).rejects.toMatchObject({
      code: "network_error",
      status: 0,
    });
    await expect(
      putChunk(
        "test",
        0,
        new Blob([]),
        "0".repeat(64),
        new AbortController().signal,
      ),
    ).rejects.toMatchObject({ code: "network_error", status: 0 });
  });
  it("keeps cancellation distinct from a lost connection", async () => {
    const aborted = new DOMException("Aborted", "AbortError");
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(aborted));
    await expect(api("/uploads")).rejects.toBe(aborted);
  });
});
