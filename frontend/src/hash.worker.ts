import { createSHA256 } from "hash-wasm";
self.onmessage = async (event: MessageEvent<{ file: File }>) => {
  try {
    const h = await createSHA256();
    h.init();
    const file = event.data.file;
    for (let p = 0; p < file.size; p += 4 * 1024 * 1024) {
      h.update(
        new Uint8Array(await file.slice(p, p + 4 * 1024 * 1024).arrayBuffer()),
      );
      self.postMessage({ progress: Math.min(file.size, p + 4 * 1024 * 1024) });
    }
    self.postMessage({ digest: h.digest("hex") });
  } catch {
    self.postMessage({ error: "hash_failed" });
  }
};
