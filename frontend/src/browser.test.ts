import { afterEach, describe, expect, it, vi } from "vitest";
import { chunkDigest, taskId } from "./browser";
const originalCrypto = globalThis.crypto;
afterEach(() => vi.unstubAllGlobals());
describe("LAN HTTP browser support", () => {
  it("creates unique task IDs without secure-context randomUUID", () => {
    vi.stubGlobal("crypto", {
      getRandomValues: originalCrypto.getRandomValues.bind(originalCrypto),
    });
    const ids = Array.from({ length: 100 }, taskId);
    expect(new Set(ids).size).toBe(100);
    expect(
      ids.every((id) =>
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
          id,
        ),
      ),
    ).toBe(true);
  });
  it("validates chunks without secure-context SubtleCrypto", async () => {
    vi.stubGlobal("crypto", {
      getRandomValues: originalCrypto.getRandomValues.bind(originalCrypto),
    });
    expect(await chunkDigest(new Blob(["abc"]))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    expect(await chunkDigest(new Blob([]))).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
  });
});
