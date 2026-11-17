import { beforeAll, describe, it, expect, vi } from "vitest";
vi.mock("./api", () => ({
  ApiError: class extends Error {
    constructor(
      public code: string,
      public status: number,
      public details: unknown,
    ) {
      super(code);
    }
  },
  api: vi.fn(),
  putChunk: vi.fn(),
}));
const digest = "a".repeat(64);
vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {} });
vi.stubGlobal("window", new EventTarget());
class HashWorker {
  onmessage: any;
  onerror: any;
  postMessage() {
    queueMicrotask(() => this.onmessage?.({ data: { digest } }));
  }
  terminate() {}
  addEventListener() {}
}
vi.stubGlobal("Worker", HashWorker);
const { api, putChunk } = await import("./api");
const {
  transfer,
  enqueue,
  CHUNK,
  pause,
  resume,
  recoverTasks,
  reselect,
  MAX_FILE,
} = await import("./transfer");
const tick = () => new Promise((resolve) => setTimeout(resolve, 5));
describe("global transfer engine", () => {
  it("limits active files to two and chunks per file to two, retaining state across navigation", async () => {
    let sequence = 0;
    const records = new Map<string, any>();
    let active = 0,
      max = 0;
    const perFile = new Map<string, number>();
    let maxPerFile = 0;
    const pending: (() => void)[] = [];
    vi.mocked(api).mockImplementation(async (path, method, data: any) => {
      if (path === "/uploads" && method === "POST") {
        const id = String(++sequence);
        const upload = { ...data, id, state: "pending" };
        records.set(id, upload);
        return { upload, chunks: [], chunk_size: CHUNK };
      }
      if (path.endsWith("/complete")) return {};
      return {
        upload: records.get(path.split("/").at(-1)!),
        chunks: [],
        chunk_size: CHUNK,
      };
    });
    vi.mocked(putChunk).mockImplementation((id: string) => {
      active++;
      max = Math.max(max, active);
      const count = (perFile.get(id) || 0) + 1;
      perFile.set(id, count);
      maxPerFile = Math.max(maxPerFile, count);
      return new Promise((resolve) =>
        pending.push(() => {
          active--;
          perFile.set(id, perFile.get(id)! - 1);
          resolve({});
        }),
      );
    });
    const file = new File([new Uint8Array(CHUNK * 2 + 1)], "large.bin");
    for (let i = 0; i < 3; i++) enqueue(file, "root");
    for (let i = 0; i < 100 && !pending.length; i++) await tick();
    expect(sequence).toBe(2);
    expect(transfer.tasks.filter((t) => t.status === "uploading").length).toBe(
      2,
    );
    window.dispatchEvent(new Event("popstate"));
    expect(transfer.tasks).toHaveLength(3);
    for (
      let i = 0;
      i < 200 && transfer.tasks.some((t) => t.status !== "complete");
      i++
    ) {
      pending.splice(0).forEach((done) => done());
      await tick();
    }
    expect(transfer.tasks.every((t) => t.status === "complete")).toBe(true);
    expect(max).toBeLessThanOrEqual(4);
    expect(maxPerFile).toBeLessThanOrEqual(2);
  });
  it("rejects oversized files and pauses queued work", () => {
    expect(() => enqueue(new File([], "too-big"), "root")).not.toThrow();
    const task = transfer.tasks.at(-1)!;
    pause(task);
    expect(task.status).toBe("paused");
    const fake = { size: MAX_FILE + 1, name: "too-big" } as File;
    expect(() => enqueue(fake, "root")).toThrow("maxSize");
    transfer.tasks = transfer.tasks.filter((t) => t !== task);
  });
  it("discovers resumable tasks and rejects different contents on reselection", async () => {
    transfer.tasks = [];
    vi.mocked(api).mockImplementation(async (path) =>
      path === "/uploads"
        ? [
            {
              id: "resume",
              parent: "root",
              name: "original",
              size: 3,
              digest: "b".repeat(64),
              strategy: "ask",
              state: "pending",
            },
          ]
        : { upload: { digest: "b".repeat(64) }, chunks: [], chunk_size: CHUNK },
    );
    await recoverTasks();
    const task = transfer.tasks[0];
    expect(task.status).toBe("recoverable");
    expect(() => reselect(task, new File(["xxxx"], "wrong"))).toThrow(
      "sourceMismatch",
    );
    reselect(task, new File(["abc"], "original"));
    for (let i = 0; i < 100 && task.status !== "failed"; i++) await tick();
    expect(task.error).toBe("sourceMismatch");
  });
});
