import { reactive } from "vue";
import { taskId, chunkDigest } from "./browser";
import { api, ApiError, putChunk, type Upload, type UploadStatus } from "./api";
export const CHUNK = 16 * 1024 * 1024,
  MAX_FILE = 10_000_000_000;
export type TaskState =
  | "queued"
  | "hashing"
  | "uploading"
  | "paused"
  | "recoverable"
  | "complete"
  | "failed"
  | "cancelled";
export interface Task {
  key: string;
  id?: string;
  name: string;
  parent: string;
  size: number;
  file?: File;
  status: TaskState;
  done: number;
  speed: number;
  error?: string;
  strategy: string;
  digest?: string;
  controller?: AbortController;
  started?: number;
  baseline?: number;
  hashProgress: number;
}
export const transfer = reactive({
  tasks: [] as Task[],
  expanded: false,
  visible: true,
});
let running = 0;
const workers = new Map<string, Worker>();
function fileHash(
  file: File,
  key: string,
  signal: AbortSignal,
): Promise<string> {
  return new Promise((resolve, reject) => {
    const w = new Worker(new URL("./hash.worker.ts", import.meta.url), {
      type: "module",
    });
    workers.set(key, w);
    const abort = () => {
      w.terminate();
      workers.delete(key);
      reject(new DOMException("Aborted", "AbortError"));
    };
    signal.addEventListener("abort", abort, { once: true });
    w.onmessage = ({ data }) => {
      const task = transfer.tasks.find((t) => t.key === key);
      if (task && data.progress) task.hashProgress = data.progress;
      if (data.digest || data.error) {
        w.terminate();
        workers.delete(key);
        signal.removeEventListener("abort", abort);
        data.digest ? resolve(data.digest) : reject(new Error(data.error));
      }
    };
    w.onerror = () => {
      w.terminate();
      workers.delete(key);
      signal.removeEventListener("abort", abort);
      reject(new Error("hash_failed"));
    };
    w.addEventListener("messageerror", () => reject(new Error("hash_failed")));
    w.postMessage({ file });
  });
}
async function retry<T>(fn: () => Promise<T>, signal: AbortSignal): Promise<T> {
  for (let i = 0; ; i++) {
    try {
      return await fn();
    } catch (e) {
      if (
        signal.aborted ||
        i >= 4 ||
        (e instanceof ApiError && e.status < 500 && e.status !== 429)
      )
        throw e;
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(
          () => {
            signal.removeEventListener("abort", abort);
            resolve();
          },
          Math.min(1000 * 2 ** i, 16000),
        );
        const abort = () => {
          clearTimeout(timer);
          reject(new DOMException("Aborted", "AbortError"));
        };
        signal.addEventListener("abort", abort, { once: true });
      });
    }
  }
}
export function enqueue(file: File, parent: string) {
  if (file.size > MAX_FILE) throw new Error("maxSize");
  transfer.tasks.push({
    key: taskId(),
    name: file.name,
    parent,
    size: file.size,
    file,
    status: "queued",
    done: 0,
    speed: 0,
    strategy: "ask",
    hashProgress: 0,
  });
  transfer.visible = true;
  pump();
}
function pump() {
  for (const task of transfer.tasks) {
    if (running >= 2) break;
    if (task.status === "queued" && task.file) {
      running++;
      void run(task).finally(() => {
        running--;
        pump();
      });
    }
  }
}
async function run(task: Task) {
  const controller = new AbortController();
  task.controller = controller;
  try {
    task.status = "hashing";
    const digest = await fileHash(task.file!, task.key, controller.signal);
    if (controller.signal.aborted) return;
    if (task.digest && digest !== task.digest)
      throw new Error("sourceMismatch");
    task.digest = digest;
    if (!task.id) {
      const data = await retry(
        () =>
          api<UploadStatus>(
            "/uploads",
            "POST",
            {
              parent: task.parent,
              name: task.name,
              size: task.size,
              digest,
              strategy: task.strategy,
            },
            controller.signal,
          ),
        controller.signal,
      );
      task.id = data.upload.id;
      task.name = data.upload.name;
    }
    const status = await api<UploadStatus>(`/uploads/${task.id}`);
    if (status.upload.digest !== digest) throw new Error("sourceMismatch");
    const done = new Set(status.chunks.map((c) => c.index));
    task.done = status.chunks.reduce((s, c) => s + c.size, 0);
    task.baseline = task.done;
    task.started = performance.now();
    task.status = "uploading";
    let next = 0;
    const worker = async () => {
      while (!controller.signal.aborted) {
        while (done.has(next)) next++;
        const index = next++;
        if (index >= Math.ceil(task.size / CHUNK)) return;
        const blob = task.file!.slice(
          index * CHUNK,
          Math.min(task.size, (index + 1) * CHUNK),
        );
        const checksum = await chunkDigest(blob);
        await retry(
          () => putChunk(task.id!, index, blob, checksum, controller.signal),
          controller.signal,
        );
        done.add(index);
        task.done += blob.size;
        task.speed =
          (task.done - task.baseline!) /
          Math.max(0.1, (performance.now() - task.started!) / 1000);
      }
    };
    const results = await Promise.allSettled([worker(), worker()]);
    const failed = results.find(
      (r): r is PromiseRejectedResult => r.status === "rejected",
    );
    if (failed) throw failed.reason;
    if (controller.signal.aborted) return;
    await retry(
      () => api(`/uploads/${task.id}/complete`, "POST", {}, controller.signal),
      controller.signal,
    );
    task.status = "complete";
    task.file = undefined;
    window.dispatchEvent(new Event("mc-files-changed"));
  } catch (e) {
    if (!controller.signal.aborted) {
      task.status = "failed";
      task.error =
        e instanceof ApiError
          ? e.code
          : e instanceof Error
            ? e.message
            : "request_failed";
    }
  } finally {
    task.controller = undefined;
  }
}
export function pause(task: Task) {
  if (["queued", "hashing", "uploading"].includes(task.status)) {
    task.status = "paused";
    task.controller?.abort();
  }
}
export function resume(task: Task, strategy?: string) {
  if (strategy) task.strategy = strategy;
  if (task.controller) return;
  if (task.file) {
    task.error = undefined;
    task.status = "queued";
    pump();
  } else task.status = "recoverable";
}
export async function cancelTask(task: Task) {
  task.controller?.abort();
  task.status = "cancelled";
  while (task.controller) await new Promise((r) => setTimeout(r, 50));
  if (task.id) {
    await retry(
      () => api(`/uploads/${task.id}`, "DELETE"),
      new AbortController().signal,
    );
  }
  task.file = undefined;
}
export async function recoverTasks() {
  const pending = await api<Upload[]>("/uploads");
  for (const u of pending) {
    if (transfer.tasks.some((t) => t.id === u.id)) continue;
    const status = await api<UploadStatus>(`/uploads/${u.id}`);
    transfer.tasks.push({
      key: taskId(),
      id: u.id,
      name: u.name,
      parent: u.parent,
      size: u.size,
      digest: u.digest,
      strategy: u.strategy,
      status: "recoverable",
      done: status.chunks.reduce((s, c) => s + c.size, 0),
      speed: 0,
      hashProgress: 0,
    });
  }
}
export function reselect(task: Task, file: File) {
  if (file.size !== task.size) throw new Error("sourceMismatch");
  task.file = file;
  resume(task);
}
export function stopAll() {
  for (const task of transfer.tasks) pause(task);
}
export function clearFinished() {
  transfer.tasks = transfer.tasks.filter(
    (t) => !["complete", "cancelled"].includes(t.status),
  );
}
