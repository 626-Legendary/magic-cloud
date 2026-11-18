// Run only against a disposable LAN deployment; this creates actual test files and shares.
import { chromium } from "playwright";
import assert from "node:assert/strict";
const base = process.env.MC_TEST_ORIGIN,
  password = process.env.MC_TEST_PASSWORD;
if (!base || !password || !base.startsWith("http://"))
  throw new Error(
    "Set MC_TEST_ORIGIN (LAN HTTP) and MC_TEST_PASSWORD for a disposable deployment",
  );
const browser = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const context = await browser.newContext({
  viewport: { width: 1280, height: 900 },
  acceptDownloads: true,
});
try {
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(base);
  await page.getByLabel("访问密码", { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.isSecureContext), false);
  assert.equal(await page.evaluate(() => typeof crypto.subtle), "undefined");
  await page.getByLabel("访问密码", { exact: true }).fill(password);
  await page.getByRole("button", { name: "进入", exact: true }).click();
  await page.locator(".app-shell").waitFor();
  const cookie = (await context.cookies()).find(
    (c) => c.name === "mc_lan_session",
  );
  assert(cookie);
  assert.equal(cookie.secure, false);
  assert.equal(cookie.httpOnly, true);
  const suffix = Date.now();
  const filename = "lan-" + suffix + ".txt";
  const content = "LAN HTTP upload\n<script>never execute</script>";
  await page
    .locator("input[type=file]")
    .nth(0)
    .setInputFiles({
      name: filename,
      mimeType: "text/plain",
      buffer: Buffer.from(content),
    });
  const row = page.locator(".file-item").filter({ hasText: filename });
  await row.waitFor({ timeout: 30000 });
  await page.reload();
  await page.locator(".app-shell").waitFor();
  await row.waitFor();
  await row.locator(".file-main").click();
  await page.waitForFunction(
    (text) => document.querySelector("dialog[open] pre")?.textContent === text,
    content,
  );
  assert.equal(await page.locator("dialog[open] pre").textContent(), content);
  await page
    .locator("dialog[open]")
    .getByRole("button", { name: "关闭", exact: true })
    .click();
  await row.locator(".file-actions > button").click();
  const downloadEvent = page.waitForEvent("download");
  await row.getByRole("menuitem", { name: "下载", exact: true }).click();
  const download = await downloadEvent;
  assert.equal(await download.failure(), null);
  const direct = await context.request.get(download.url());
  assert.equal(direct.status(), 200);
  assert.equal((await direct.body()).toString(), content);
  if (!(await row.locator(".context-menu").isVisible()))
    await row.locator(".file-actions > button").click();
  await row.getByRole("menuitem", { name: "分享", exact: true }).click();
  await page
    .locator("dialog[open]")
    .getByRole("button", { name: "确认", exact: true })
    .click();
  const input = page.locator("dialog[open] input[readonly]");
  await input.waitFor();
  const shared = await input.inputValue();
  assert(shared.startsWith(base + "/s/"));
  assert.equal(
    await page.evaluate(() => typeof navigator.clipboard),
    "undefined",
  );
  await page
    .locator("dialog[open]")
    .getByRole("button", { name: "复制链接", exact: true })
    .click();
  await page.getByRole("status").filter({ hasText: "链接已复制" }).waitFor();
  await page.keyboard.press("Escape");
  const sharedContext = await browser.newContext();
  try {
    const sharedPage = await sharedContext.newPage();
    await sharedPage.goto(shared);
    await sharedPage.locator(".public-download-card").waitFor();
    assert.equal(await sharedPage.locator(".sidebar").count(), 0);
  } finally {
    await sharedContext.close();
  }
  await page.route("**/api/uploads/*/chunks/*", async (r) => {
    await new Promise((resolve) => setTimeout(resolve, 250));
    await r.continue().catch(() => {});
  });
  const started = page.waitForRequest(
    (r) => r.method() === "PUT" && r.url().includes("/chunks/"),
  );
  const completed = page.waitForResponse(
    (r) => r.request().method() === "POST" && r.url().endsWith("/complete"),
  );
  await page
    .locator("input[type=file]")
    .nth(0)
    .setInputFiles({
      name: "lan-large-" + suffix + ".bin",
      mimeType: "application/octet-stream",
      buffer: Buffer.alloc(17 * 1024 * 1024, 7),
    });
  await started;
  await page.getByRole("link", { name: "设置", exact: true }).click();
  assert.equal((await completed).status(), 200);
  await page.unroute("**/api/uploads/*/chunks/*");
  await page.getByRole("button", { name: "退出登录", exact: true }).click();
  await page.getByLabel("访问密码", { exact: true }).waitFor();
  assert.equal(
    (await context.cookies()).some((c) => c.name === "mc_lan_session"),
    false,
  );
  assert.deepEqual(errors, []);
  console.log(
    "PASS: non-secure LAN HTTP login/reload/logout, real upload/download/preview, copy/share and 17 MiB upload across routes",
  );
} finally {
  await context.close();
  await browser.close();
}
