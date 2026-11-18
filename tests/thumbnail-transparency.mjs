// Creates one image only in a disposable deployment; checks actual generated pixels.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
const base = process.env.MC_TEST_ORIGIN,
  password = process.env.MC_TEST_PASSWORD;
if (process.env.MC_TEST_DISPOSABLE !== "1" || !base || !password)
  throw new Error(
    "Set disposable test origin/password and MC_TEST_DISPOSABLE=1",
  );
const browser = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const context = await browser.newContext({
  viewport: { width: 1280, height: 900 },
});
try {
  const p = await context.newPage();
  await p.goto(base);
  await p.getByLabel("访问密码", { exact: true }).fill(password);
  await p.getByRole("button", { name: "进入", exact: true }).click();
  await p.locator(".app-shell").waitFor();
  const name = `transparent-${Date.now()}.png`;
  await p
    .locator("input[type=file]")
    .first()
    .setInputFiles({
      name,
      mimeType: "image/png",
      buffer: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAAp0lEQVR4nO3QwQmAMAAEwTRgq9Zg46JFRBmD+5jvcezY9nP8GT+g8QMaP6DxAxo/oPEDGj+g8QMaP6DxAxo/oPEDGj+g8QMaP6A9MXJgBShAAQpQgAIU4LsBrkkFKEABClCAAhSgAAUoQAEKUIACFGC9AG8rQAEKUIACFKAALsDS+AGNH9D4AY0f0PgBjR/Q+AGNH9D4AY0f0PgBjR/Q+AGNH9D4Ae0GExYAhyg7NVAAAAAASUVORK5CYII=",
        "base64",
      ),
    });
  const row = p.locator(".file-item").filter({ hasText: name });
  await row.waitFor();
  const img = row.locator(".media-thumbnail img");
  await img.waitFor({ timeout: 30000 });
  await img.evaluate((e) => e.decode());
  const result = await img.evaluate((e) => {
    const c = document.createElement("canvas");
    c.width = e.naturalWidth;
    c.height = e.naturalHeight;
    const ctx = c.getContext("2d");
    ctx.drawImage(e, 0, 0);
    return {
      corner: Array.from(ctx.getImageData(0, 0, 1, 1).data),
      center: Array.from(
        ctx.getImageData(
          Math.floor(c.width / 2),
          Math.floor(c.height / 2),
          1,
          1,
        ).data,
      ),
    };
  });
  assert.equal(
    result.corner[3],
    0,
    "transparent pixels must retain zero alpha",
  );
  assert.deepEqual(result.center, [13, 110, 253, 255]);
  assert.equal(
    await row
      .locator(".media-artwork")
      .evaluate((e) => getComputedStyle(e).visibility),
    "hidden",
  );
  const nodes = await (
    await context.request.get(base + "/api/nodes?parent=root")
  ).json();
  const node = nodes.nodes.find((n) => n.name === name);
  const response = await context.request.get(
    base + `/api/nodes/${node.id}/thumbnail`,
  );
  assert.equal(response.headers()["content-type"], "image/png");
  assert.deepEqual(
    Array.from((await response.body()).subarray(0, 8)),
    [137, 80, 78, 71, 13, 10, 26, 10],
  );
  if (process.env.MC_TEST_ARTIFACTS) {
    mkdirSync(process.env.MC_TEST_ARTIFACTS, { recursive: true });
    await p.screenshot({
      path: join(process.env.MC_TEST_ARTIFACTS, "alpha-light.png"),
      fullPage: true,
    });
    await p.getByRole("button", { name: "外观", exact: true }).click();
    await p.setViewportSize({ width: 320, height: 900 });
    await p.screenshot({
      path: join(process.env.MC_TEST_ARTIFACTS, "alpha-dark-mobile.png"),
      fullPage: true,
    });
  }
  const { csrf } = await (
    await context.request.get(base + "/api/auth/session")
  ).json();
  const deleted = await context.request.delete(base + `/api/nodes/${node.id}`, {
    headers: { Origin: base, "X-CSRF-Token": csrf },
  });
  assert.equal(deleted.status(), 200);
  console.log(
    "PASS actual PNG thumbnail keeps alpha=0 in transparent areas and exact opaque color, correct MIME/header, light/dark mobile display",
  );
} finally {
  await context.close();
  await browser.close();
}
