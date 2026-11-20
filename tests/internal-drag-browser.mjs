// All API requests are mocked; mouse gestures cannot modify real files.
import { chromium } from "playwright";
import assert from "node:assert/strict";
const base = process.env.MC_TEST_ORIGIN;
if (!base) throw new Error("Set MC_TEST_ORIGIN");
const browser = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const context = await browser.newContext({
  viewport: { width: 1280, height: 900 },
});
try {
  const logo = await context.request.get(base + "/Logo.png");
  const image = await logo.body();
  const nodes = [
    {
      id: "picture",
      name: "a.png",
      kind: "file",
      size: image.length,
      mtime: 1700000000,
    },
    {
      id: "document",
      name: "b.docx",
      kind: "file",
      size: 1,
      mtime: 1700000000,
    },
  ];
  const uploads = new Map();
  let uploadRequests = 0;
  await context.route("**/api/**", async (route) => {
    const request = route.request(),
      path = new URL(request.url()).pathname;
    if (path.endsWith("/thumbnail") || path.endsWith("/download")) {
      await route.fulfill({ contentType: "image/png", body: image });
      return;
    }
    let data = [];
    if (path === "/api/auth/session") data = { csrf: "fixture-only" };
    else if (path === "/api/nodes")
      data = { nodes, breadcrumbs: [{ id: "root", name: "Root" }] };
    else if (path === "/api/settings")
      data = { storage: { used: 1, total: 100 }, system: { version: "1.0.1" } };
    else if (path === "/api/uploads" && request.method() === "POST") {
      uploadRequests++;
      const upload = {
        ...request.postDataJSON(),
        id: `upload-${uploadRequests}`,
        expires: 4102444800,
        state: "pending",
      };
      uploads.set(upload.id, upload);
      data = { upload, chunk_size: 16777216, chunks: [] };
    } else if (/^\/api\/uploads\/[^/]+$/.test(path))
      data = {
        upload: uploads.get(path.split("/").at(-1)),
        chunks: [],
        chunk_size: 16777216,
      };
    else if (path.endsWith("/complete")) {
      const upload = uploads.get(path.split("/").at(-2));
      const node = {
        id: upload.id,
        name: upload.name,
        kind: "file",
        size: upload.size,
        mtime: 1700000000,
      };
      nodes.push(node);
      data = node;
    }
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify(data),
    });
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(base + "/files/root");
  const row = page
    .locator(".file-item")
    .filter({ has: page.getByRole("button", { name: "a.png", exact: true }) });
  const thumbnail = row.locator(".media-thumbnail img");
  await thumbnail.waitFor();
  await thumbnail.evaluate((img) => img.decode());
  assert.equal(await thumbnail.evaluate((img) => img.draggable), false);
  const original = await row.boundingBox();
  const box = await thumbnail.boundingBox();
  for (let i = 0; i < 3; i++) {
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(150);
    await page.mouse.move(
      box.x + box.width / 2 + 40,
      box.y + box.height / 2 + 30,
      { steps: 8 },
    );
    assert.equal(await page.locator(".drop-overlay").count(), 0);
    assert.equal(await page.evaluate(() => getSelection().toString()), "");
    const held = await row.boundingBox();
    assert.equal(held.x, original.x);
    assert.equal(held.y, original.y);
    await page.mouse.up();
    assert.equal(
      await page.locator(".preview-dialog[open]").count(),
      0,
      "dragging must not click through to preview",
    );
  }
  assert.equal(uploadRequests, 0);
  await row.locator(".file-main").focus();
  await page.keyboard.press("Enter");
  await page.locator(".preview-dialog[open]").waitFor();
  await page.keyboard.press("Escape");
  await row.locator(".file-selection").check();
  assert.equal(await row.locator(".file-selection").isChecked(), true);
  await row.locator(".file-selection").uncheck();
  await row.locator(".file-actions > button").click();
  await row.getByRole("menuitem", { name: "预览", exact: true }).waitFor();
  await page.keyboard.press("Escape");
  await row.dblclick({ position: { x: 6, y: 6 } });
  await page.locator(".preview-dialog[open]").waitFor();
  await page.keyboard.press("Escape");
  const cancelled = await thumbnail.evaluate(
    (img) =>
      !img.dispatchEvent(
        new DragEvent("dragstart", {
          bubbles: true,
          cancelable: true,
          dataTransfer: new DataTransfer(),
        }),
      ),
  );
  assert.equal(cancelled, true, "file cards must cancel native drag images");
  await page.evaluate(() => {
    const payload = new DataTransfer();
    payload.items.add(new File(["thumbnail"], "a.png", { type: "image/png" }));
    document.querySelector(".brand img").dispatchEvent(
      new DragEvent("dragstart", {
        bubbles: true,
        cancelable: true,
        dataTransfer: payload,
      }),
    );
    window.dispatchEvent(new Event("blur"));
    const area = document.querySelector(".content");
    area.dispatchEvent(
      new DragEvent("dragover", {
        bubbles: true,
        cancelable: true,
        dataTransfer: payload,
      }),
    );
    area.dispatchEvent(
      new DragEvent("drop", {
        bubbles: true,
        cancelable: true,
        dataTransfer: payload,
      }),
    );
  });
  await page.waitForTimeout(200);
  assert.equal(uploadRequests, 0);
  assert.equal(
    await page.locator(".transfer-panel").count(),
    0,
    "internal drags must not enqueue thumbnail files",
  );
  assert.equal(await page.locator(".drop-overlay").count(), 0);
  await page.locator(".content").evaluate((area) => {
    const payload = new DataTransfer();
    payload.items.add(
      new File(["External file"], "outside.txt", { type: "text/plain" }),
    );
    area.dispatchEvent(
      new DragEvent("dragover", {
        bubbles: true,
        cancelable: true,
        dataTransfer: payload,
      }),
    );
  });
  await page.locator(".drop-overlay").waitFor();
  await page.locator(".content").evaluate((area) => {
    const payload = new DataTransfer();
    payload.items.add(
      new File(["External file"], "outside.txt", { type: "text/plain" }),
    );
    area.dispatchEvent(
      new DragEvent("drop", {
        bubbles: true,
        cancelable: true,
        dataTransfer: payload,
      }),
    );
  });
  await page
    .getByRole("button", { name: "outside.txt", exact: true })
    .waitFor();
  assert.equal(uploadRequests, 1);
  assert.equal(await page.locator(".drop-overlay").count(), 0);
  await page.getByRole("button", { name: "a.png", exact: true }).click();
  await page.locator(".preview-dialog[open]").waitFor();
  await page.keyboard.press("Escape");
  await page
    .getByRole("button", { name: "切换视图 list", exact: true })
    .click();
  for (const name of ["a.png", "b.docx"]) {
    const button = page.getByRole("button", { name, exact: true });
    const bounds = await button.boundingBox();
    await page.mouse.move(bounds.x + 50, bounds.y + bounds.height / 2);
    await page.mouse.down();
    await page.mouse.move(bounds.x + 85, bounds.y + bounds.height / 2 + 3, {
      steps: 5,
    });
    await page.mouse.up();
    assert.equal(await page.locator(".preview-dialog[open]").count(), 0);
    assert.equal(await page.locator(".drop-overlay").count(), 0);
    assert.equal(uploadRequests, 1);
  }
  assert.deepEqual(errors, []);
  console.log(
    "PASS native thumbnail/card drag blocked, no selection/ghost/click-through or duplicate uploads, internal payload blocked after blur, external file upload and normal preview preserved",
  );
} finally {
  await context.close();
  await browser.close();
}
