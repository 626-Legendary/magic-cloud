// Read-only regression: all API calls are intercepted.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
const base = process.env.MC_TEST_ORIGIN;
if (!base) throw new Error("Set MC_TEST_ORIGIN");
const browser = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const context = await browser.newContext({
  viewport: { width: 1280, height: 900 },
});
let releaseSession;
const sessionGate = new Promise((resolve) => {
  releaseSession = resolve;
});
const node = {
  id: "fixture",
  name: "example.txt",
  kind: "file",
  size: 12,
  mtime: 1700000000,
};
const artifacts = process.env.MC_TEST_ARTIFACTS;
if (artifacts) mkdirSync(artifacts, { recursive: true });
try {
  await context.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    let data = [];
    if (path === "/api/auth/session") {
      await sessionGate;
      data = { csrf: "fixture-only" };
    } else if (path === "/api/nodes")
      data = { nodes: [node], breadcrumbs: [{ id: "root", name: "Root" }] };
    else if (path === "/api/settings")
      data = { storage: { used: 1, total: 100 }, system: { version: "1.0.1" } };
    else if (path.startsWith("/api/public/"))
      data = { root: node, node, nodes: [] };
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify(data),
    });
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(base + "/files/root");
  await page.locator(".boot img").evaluate((image) => image.decode());
  for (const width of [1280, 320]) {
    await page.setViewportSize({ width, height: 900 });
    assert.equal((await page.locator(".boot img").boundingBox()).width, 160);
    assert.equal(
      await page
        .locator(".boot .spinner")
        .evaluate((element) => getComputedStyle(element).width),
      "32px",
    );
    assert.equal(await page.locator(".boot").getAttribute("role"), "status");
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth,
      ),
      false,
    );
    if (artifacts)
      await page.screenshot({ path: join(artifacts, `loading-${width}.png`) });
  }
  releaseSession();
  await page.locator(".file-main").waitFor();
  await page.setViewportSize({ width: 1280, height: 900 });
  async function over(selector = ".file-main", text = false) {
    await page.locator(selector).evaluate((element, text) => {
      const transfer = new DataTransfer();
      if (text) transfer.setData("text/plain", "text drag");
      else transfer.items.add(new File(["test"], "test.txt"));
      const rect = element.getBoundingClientRect();
      element.dispatchEvent(
        new DragEvent("dragover", {
          bubbles: true,
          cancelable: true,
          dataTransfer: transfer,
          clientX: rect.x + rect.width / 2,
          clientY: rect.y + rect.height / 2,
        }),
      );
    }, text);
  }
  const overlay = page.locator(".drop-overlay");
  await over(".file-main", true);
  assert.equal(await overlay.count(), 0);
  await over();
  await overlay.waitFor();
  await page.locator(".file-main").evaluate((element) =>
    element.dispatchEvent(
      new DragEvent("dragleave", {
        bubbles: true,
        relatedTarget: document.querySelector(".page-heading"),
      }),
    ),
  );
  assert.equal(
    await overlay.count(),
    1,
    "crossing child elements must keep the overlay",
  );
  await page.keyboard.press("Escape");
  await overlay.waitFor({ state: "hidden" });
  for (const event of ["dragend", "drop", "blur", "pointerdown"]) {
    await over();
    await overlay.waitFor();
    await page.evaluate(
      (event) => window.dispatchEvent(new Event(event)),
      event,
    );
    await overlay.waitFor({ state: "hidden" });
  }
  await over();
  await overlay.waitFor();
  await over(".sidebar");
  await overlay.waitFor({ state: "hidden" });
  await over();
  await overlay.waitFor();
  await page
    .locator(".file-main")
    .evaluate((element) =>
      element.dispatchEvent(
        new DragEvent("dragleave", { bubbles: true, clientX: 0, clientY: 0 }),
      ),
    );
  await overlay.waitFor({ state: "hidden" });
  await over();
  await overlay.waitFor();
  for (let i = 0; i < 3; i++) {
    await page.waitForTimeout(600);
    await over();
    assert.equal(await overlay.count(), 1);
  }
  await overlay.waitFor({ state: "hidden", timeout: 3500 });
  await over();
  await overlay.waitFor();
  await page.getByRole("link", { name: "最近文件", exact: true }).click();
  await overlay.waitFor({ state: "hidden" });
  await page.goto(base + "/s/fixture");
  await page.locator(".public-download-card").waitFor();
  assert.equal(
    await page
      .locator(".share-lock")
      .evaluate((element) => getComputedStyle(element).color),
    "rgb(251, 191, 36)",
  );
  assert.deepEqual(errors, []);
  console.log(
    "PASS large responsive loading, file-only drag overlay, nested movement, cancel/end/drop/blur/outside/idle/route cleanup and yellow sharing lock",
  );
} finally {
  releaseSession();
  await context.close();
  await browser.close();
}
