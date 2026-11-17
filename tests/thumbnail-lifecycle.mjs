// Only for a disposable deployment and its mounted test storage; creates test videos.
import assert from "node:assert/strict";
import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
const base = process.env.MC_TEST_ORIGIN;
const password = process.env.MC_TEST_PASSWORD;
const storage = process.env.MC_TEST_STORAGE;
const video = process.env.MC_TEST_VIDEO;
if (
  process.env.MC_TEST_DISPOSABLE !== "1" ||
  !base ||
  !password ||
  !storage ||
  !video
)
  throw new Error(
    "Set MC_TEST_DISPOSABLE=1, MC_TEST_ORIGIN, MC_TEST_PASSWORD, MC_TEST_STORAGE and MC_TEST_VIDEO for a disposable deployment.",
  );
const directory = `${storage}/.magic-cloud/thumbnails`;
const log = `${storage}/.magic-cloud/ffmpeg-invocations`;
const files = (extension) =>
  readdirSync(directory).filter((n) => n.endsWith(extension));
const renders = () =>
  existsSync(log)
    ? readFileSync(log, "utf8").trim().split("\n").filter(Boolean).length
    : undefined;
async function until(check) {
  const deadline = Date.now() + 30000;
  while (!check()) {
    if (Date.now() > deadline)
      throw new Error("Timed out waiting for the background thumbnail job");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}
const login = await fetch(base + "/api/auth/login", {
  method: "POST",
  headers: { Origin: base, "Content-Type": "application/json" },
  body: JSON.stringify({ password }),
});
assert.equal(login.status, 200);
const cookie = login.headers.get("set-cookie").split(";")[0];
const { csrf } = await login.json();
const headers = {
  Cookie: cookie,
  Origin: base,
  "X-CSRF-Token": csrf,
  "Content-Type": "application/json",
};
async function api(path, method = "GET", data) {
  const response = await fetch(base + "/api" + path, {
    method,
    headers,
    body: data === undefined ? undefined : JSON.stringify(data),
  });
  assert(response.ok, `${method} ${path}: ${response.status}`);
  return response.json();
}
async function upload(bytes, name) {
  assert(
    bytes.length < 16 * 1024 * 1024,
    "Use a short fixture video under 16 MiB",
  );
  const digest = createHash("sha256").update(bytes).digest("hex");
  const { upload } = await api("/uploads", "POST", {
    parent: "root",
    name,
    size: bytes.length,
    digest,
    strategy: "ask",
  });
  const chunk = await fetch(base + `/api/uploads/${upload.id}/chunks/0`, {
    method: "PUT",
    headers: {
      ...headers,
      "Content-Type": "application/octet-stream",
      "X-Chunk-SHA256": digest,
    },
    body: bytes,
  });
  assert.equal(chunk.status, 200);
  return api(`/uploads/${upload.id}/complete`, "POST", {});
}
const prefix = `thumbnail-lifecycle-${Date.now()}`;
const before = new Set(files(".jpg"));
const count = renders();
const node = await upload(readFileSync(video), prefix + ".mp4");
// Deliberately never request /thumbnail until the background job has written its output.
await until(() => files(".jpg").some((n) => !before.has(n)));
const cached = files(".jpg").find((n) => !before.has(n));
const modified = statSync(`${directory}/${cached}`).mtimeMs;
for (let i = 0; i < 3; i++) {
  const response = await fetch(base + `/api/nodes/${node.id}/thumbnail`, {
    headers,
  });
  assert.equal(response.status, 200);
  const bytes = new Uint8Array(await response.arrayBuffer());
  assert.equal(bytes[0], 255);
  assert.equal(bytes[1], 216);
}
assert.equal(statSync(`${directory}/${cached}`).mtimeMs, modified);
if (count !== undefined) assert.equal(renders(), count + 1);
await api(`/nodes/${node.id}`, "DELETE");
assert(!existsSync(`${directory}/${cached}`));
const failures = new Set(files(".failed"));
const bad = await upload(
  Buffer.from("invalid movie fixture"),
  prefix + "-invalid.mp4",
);
await until(() => files(".failed").some((n) => !failures.has(n)));
const marker = files(".failed").find((n) => !failures.has(n));
const failedRenders = renders();
for (let i = 0; i < 2; i++)
  assert.equal(
    (await fetch(base + `/api/nodes/${bad.id}/thumbnail`, { headers })).status,
    400,
  );
if (failedRenders !== undefined) assert.equal(renders(), failedRenders);
await api(`/nodes/${bad.id}`, "DELETE");
assert(!existsSync(`${directory}/${marker}`));
console.log(
  "PASS proactive generation without thumbnail requests, cached reuse, failure state, and cache cleanup on deletion",
);
