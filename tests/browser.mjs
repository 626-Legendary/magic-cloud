// Run against a disposable test deployment. Requires Playwright and an isolated Chromium CDP endpoint.
if (!process.env.MC_TEST_ORIGIN || !process.env.MC_TEST_PASSWORD)
  throw new Error(
    "Set MC_TEST_ORIGIN and MC_TEST_PASSWORD for a disposable deployment",
  );
import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const context = await browser.newContext({
  ignoreHTTPSErrors: true,
  viewport: { width: 1440, height: 1000 },
  permissions: ["clipboard-read", "clipboard-write"],
});
const suffix = Date.now();
const folderName = "Browser QA " + suffix;
const fileName = "qa-" + suffix + ".txt";
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("401")) errors.push(m.text());
});
await page.goto(process.env.MC_TEST_ORIGIN);
await page.getByLabel("访问密码", { exact: true }).waitFor();
await page.screenshot({ path: "/tmp/mc-login.png", fullPage: true });
await page
  .getByLabel("访问密码", { exact: true })
  .fill(process.env.MC_TEST_PASSWORD);
await page.getByRole("button", { name: "打开我的云空间", exact: true }).click();
await page.locator(".app-shell").waitFor();
await page.getByRole("button", { name: "新建文件夹", exact: true }).click();
await page.locator("dialog[open] input").fill(folderName);
await page
  .locator("dialog[open]")
  .getByRole("button", { name: "确认", exact: true })
  .click();
await page.locator(".file-item").filter({ hasText: folderName }).waitFor();
await page
  .locator("input[type=file]")
  .nth(0)
  .setInputFiles({
    name: fileName,
    mimeType: "text/plain",
    buffer: Buffer.from('Hello Magic Cloud\n<script>alert("never")</script>'),
  });
await page
  .locator(".file-item")
  .filter({ hasText: fileName })
  .waitFor({ timeout: 30000 });
await page.locator(".transfer-title").click();
await page.locator(".task-state.complete").waitFor();
await page.locator(".transfer-title").click();
const row = page.locator(".file-item").filter({ hasText: fileName });
await row.locator(".file-main").click();
await page.locator("dialog[open] pre").waitFor();
assert.match(await page.locator("dialog[open] pre").innerText(), /<script>/);
await page
  .locator("dialog[open]")
  .getByRole("button", { name: "关闭", exact: true })
  .click();
await row.locator(".file-actions > button").click();
await row.getByRole("button", { name: "分享", exact: true }).click();
await page
  .locator("dialog[open]")
  .getByRole("button", { name: "确认", exact: true })
  .click();
await page.locator("dialog[open] input[readonly]").waitFor();
const shareUrl = await page
  .locator("dialog[open] input[readonly]")
  .inputValue();
await page.keyboard.press("Escape");
const publicContext = await browser.newContext({ ignoreHTTPSErrors: true });
const publicPage = await publicContext.newPage();
await publicPage.goto(shareUrl);
await publicPage.locator(".public-single").waitFor();
assert.equal(await publicPage.locator(".sidebar").count(), 0);
assert.equal(await publicPage.locator("video").count(), 0);
await publicContext.close();
await page.screenshot({ path: "/tmp/mc-files-desktop.png", fullPage: true });
await page.getByRole("link", { name: "设置", exact: true }).click();
await page.locator(".settings-grid").waitFor();
await page.locator(".setting-row select").nth(0).selectOption("en");
await page.locator(".setting-row select").nth(1).selectOption("dark");
assert.equal(await page.locator("html").getAttribute("data-theme"), "dark");
await page.screenshot({ path: "/tmp/mc-settings-dark.png", fullPage: true });
await page.setViewportSize({ width: 390, height: 844 });
await page
  .getByRole("button", { name: "Open navigation", exact: true })
  .click();
await page.getByRole("link", { name: "My files", exact: true }).click();
await page.locator(".file-item").first().waitFor();
await page.waitForTimeout(250);
await page.screenshot({ path: "/tmp/mc-files-mobile.png", fullPage: true });
assert.equal(
  await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
  false,
);
assert.deepEqual(errors, []);
console.log(
  "PASS: HTTPS login, directory creation, real Worker/chunk upload, escaped preview, public share, theme/language, responsive layout; no browser errors",
);
await context.close();
await browser.close();
