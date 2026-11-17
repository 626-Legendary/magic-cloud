// Run only against a fresh, disposable deployment. Creates actual files and upload tasks.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
const base = process.env.MC_TEST_ORIGIN,
  password = process.env.MC_TEST_PASSWORD;
if (
  process.env.MC_TEST_DISPOSABLE !== "1" ||
  !base?.startsWith("http://") ||
  !password
)
  throw new Error(
    "Set MC_TEST_DISPOSABLE=1, MC_TEST_ORIGIN and MC_TEST_PASSWORD for a fresh disposable LAN deployment. This test uploads files.",
  );
const artifacts =
  process.env.MC_TEST_ARTIFACTS || "/tmp/magic-cloud-design-review";
mkdirSync(artifacts, { recursive: true });
const b = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const c = await b.newContext({ viewport: { width: 1440, height: 1000 } });
const p = await c.newPage();
const errors = [];
p.on("pageerror", (e) => errors.push(e.message));
const result = {};
function lum(rgb) {
  const vals = rgb
    .match(/[\d.]+/g)
    .slice(0, 3)
    .map((v) => +v / 255)
    .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return vals.reduce((s, v, i) => s + v * [0.2126, 0.7152, 0.0722][i], 0);
}
function contrast(a, b) {
  const [l, h] = [lum(a), lum(b)].sort((a, b) => a - b);
  return (h + 0.05) / (l + 0.05);
}
try {
  await p.goto(base);
  await p.locator(".login-form").waitFor();
  await p.locator(".hero-image").evaluate((i) => i.decode());
  await p.screenshot({
    path: join(artifacts, "design-fixed-login.png"),
    fullPage: true,
  });
  await p.getByLabel("访问密码", { exact: true }).fill(password);
  await p.getByRole("button", { name: "打开我的云空间", exact: true }).click();
  await p.locator(".app-shell").waitFor();
  await p
    .getByRole("heading", {
      name: "这里还是空空的，上传第一份文件吧！",
      exact: true,
    })
    .waitFor();
  await p.getByRole("button", { name: "新建文件夹", exact: true }).click();
  assert(
    (await p.getByRole("dialog").ariaSnapshot()).includes(
      'dialog "新建文件夹"',
    ),
  );
  assert.equal(
    await p
      .locator("dialog[open] input")
      .evaluate((e) => e === document.activeElement),
    true,
  );
  await p.locator("dialog[open] input").fill("Design folder");
  await p
    .locator("dialog[open]")
    .getByRole("button", { name: "确认", exact: true })
    .click();
  await p.locator(".file-item").filter({ hasText: "Design folder" }).waitFor();
  let release;
  const hold = new Promise((r) => (release = r));
  let chunkStarted;
  const started = new Promise((r) => (chunkStarted = r));
  await p.route("**/chunks/**", async (r) => {
    chunkStarted();
    await hold;
    await r.continue();
  });
  const chooser = p.waitForEvent("filechooser");
  await p
    .getByRole("button", { name: "拖拽文件到此处，或点击上传", exact: true })
    .click();
  await (
    await chooser
  ).setFiles([
    {
      name: "audit.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("Design verification"),
    },
    {
      name: "REPORT.PDF",
      mimeType: "application/pdf",
      buffer: Buffer.from("PDF type fixture"),
    },
    {
      name: "Plan.DOCX",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("Word type fixture"),
    },
    {
      name: "Budget.XLSX",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("Excel type fixture"),
    },
    {
      name: "archive.ZIP",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("Unknown type fixture"),
    },
  ]);
  await started;
  await p.locator(".transfer-title").click();
  await p.locator(".transfer-panel .meter[role=progressbar]").first().waitFor();
  assert.equal(
    await p.locator(".upload-dropzone [role=progressbar]").count(),
    1,
  );
  release();
  await p.waitForFunction(
    () => document.querySelectorAll(".file-item").length === 6,
  );
  await p
    .getByRole("status")
    .filter({ hasText: "上传成功！文件已经安顿好啦。" })
    .last()
    .waitFor();
  await p.unroute("**/chunks/**");
  await p.locator(".transfer-title").click();
  for (const cls of ["pdf", "word", "excel", "unknown"])
    assert.equal(await p.locator(".file-item." + cls).count(), 1);
  assert.equal(await p.locator(".file-grid").count(), 1);
  assert.equal(
    await p.locator(".file-grid .file-date").first().isVisible(),
    true,
  );
  const square = await p.locator(".file-symbol").first().boundingBox();
  assert(Math.abs(square.width - square.height) < 1);
  await p.keyboard.press("/");
  assert.equal(
    await p
      .locator(".search input")
      .evaluate((e) => e === document.activeElement),
    true,
  );
  const focus = await p
    .locator(".search input")
    .evaluate((e) => ({
      outline: getComputedStyle(e).outlineStyle,
      color: getComputedStyle(e).outlineColor,
      shadow: getComputedStyle(e).boxShadow,
    }));
  assert.notEqual(focus.outline, "none");
  assert.notEqual(focus.shadow, "none");
  result.focus = focus;
  await p.locator(".search input").fill("audit");
  await p.getByRole("button", { name: "清除搜索", exact: true }).click();
  assert.equal(await p.locator(".search input").inputValue(), "");
  const firstSelect = p.locator(".file-item .file-selection").first();
  await firstSelect.focus();
  await p.keyboard.press("Space");
  assert.equal(await firstSelect.isChecked(), true);
  assert.equal(await p.locator(".file-item.is-selected").count(), 1);
  await p.getByRole("button", { name: "取消选择", exact: true }).click();
  assert.equal(await firstSelect.isChecked(), false);
  await p.getByRole("button", { name: "切换视图 list", exact: true }).click();
  assert.equal(
    await p
      .getByRole("button", { name: "切换视图 list", exact: true })
      .getAttribute("aria-pressed"),
    "true",
  );
  await p.getByRole("checkbox", { name: "选择全部文件", exact: true }).check();
  assert.equal(await p.locator(".file-item.is-selected").count(), 6);
  await p.getByRole("button", { name: "取消选择", exact: true }).click();
  const row = await p.locator(".file-item").first().boundingBox(),
    head = await p.locator(".list-head").boundingBox();
  result.list = { rowHeight: row.height, headerHeight: head.height };
  assert.equal(row.height, 48);
  assert.equal(head.height, 40);
  await p.screenshot({
    path: join(artifacts, "design-fixed-list.png"),
    fullPage: true,
  });
  await p.getByRole("button", { name: "切换视图 grid", exact: true }).click();
  const menuButton = p.getByRole("button", {
    name: "文件操作 audit.txt",
    exact: true,
  });
  await menuButton.click();
  await p.getByRole("menu").waitFor();
  assert.equal(
    await p
      .getByRole("menuitem")
      .first()
      .evaluate((e) => e === document.activeElement),
    true,
  );
  await p.keyboard.press("ArrowDown");
  assert.equal(
    await p
      .getByRole("menuitem")
      .nth(1)
      .evaluate((e) => e === document.activeElement),
    true,
  );
  await p.keyboard.press("Escape");
  await p.waitForFunction(
    () =>
      document.activeElement?.getAttribute("aria-label") ===
      "文件操作 audit.txt",
  );
  assert.equal(
    await menuButton.evaluate((e) => e === document.activeElement),
    true,
  );
  await menuButton.click();
  await p.getByRole("menuitem", { name: "预览", exact: true }).click();
  await p.getByRole("dialog", { name: "audit.txt", exact: true }).waitFor();
  await p.getByText("Design verification", { exact: true }).waitFor();
  await p.keyboard.press("Escape");
  await p.waitForFunction(
    () =>
      document.activeElement?.getAttribute("aria-label") ===
      "文件操作 audit.txt",
  );
  assert.equal(
    await menuButton.evaluate((e) => e === document.activeElement),
    true,
  );
  await menuButton.click();
  await p.getByRole("menuitem", { name: "移动", exact: true }).click();
  await p
    .getByRole("dialog", { name: "选择目标文件夹", exact: true })
    .waitFor();
  assert.equal(
    await p
      .getByText("选择目标文件夹，再确认移动。", { exact: true })
      .isVisible(),
    true,
  );
  await p.keyboard.press("Escape");
  await p.waitForFunction(
    () =>
      document.activeElement?.getAttribute("aria-label") ===
      "文件操作 audit.txt",
  );
  assert.equal(
    await menuButton.evaluate((e) => e === document.activeElement),
    true,
  );
  await p.getByRole("link", { name: "设置", exact: true }).click();
  await p
    .getByRole("combobox", { name: "外观", exact: true })
    .selectOption("dark");
  result.settingsLabels = await p
    .locator(".setting-row select")
    .evaluateAll((es) => es.map((e) => e.labels.length));
  assert.deepEqual(result.settingsLabels, [1, 1]);
  const nav = await p
    .locator(".sidebar nav a.active")
    .evaluate((e) => ({
      fg: getComputedStyle(e).color,
      bg: getComputedStyle(e).backgroundColor,
    }));
  result.darkNavContrast = contrast(nav.fg, nav.bg);
  assert(result.darkNavContrast >= 4.5);
  await p.getByRole("link", { name: "我的文件", exact: true }).click();
  const button = await p
    .locator(".heading-actions .primary")
    .evaluate((e) => ({
      fg: getComputedStyle(e).color,
      bg: getComputedStyle(e).backgroundColor,
    }));
  result.darkButtonContrast = contrast(button.fg, button.bg);
  assert(result.darkButtonContrast >= 4.5);
  await p.screenshot({
    path: join(artifacts, "design-fixed-dark.png"),
    fullPage: true,
  });
  await p.setViewportSize({ width: 320, height: 800 });
  await p.waitForTimeout(300);
  assert.equal(
    await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
    false,
  );
  assert.equal(await p.locator(".sidebar").getAttribute("inert"), "");
  await p.locator(".search input").focus();
  await p.keyboard.press("Shift+Tab");
  assert.equal(
    await p
      .locator(".mobile-only")
      .evaluate((e) => e === document.activeElement),
    true,
  );
  const navButton = p.getByRole("button", { name: "打开导航", exact: true });
  await navButton.click();
  await p.getByRole("dialog", { name: "主导航", exact: true }).waitFor();
  assert.equal(
    await p
      .locator(".sidebar")
      .evaluate((e) => e.contains(document.activeElement)),
    true,
  );
  assert.equal(await p.locator(".workspace").getAttribute("inert"), "");
  const items = p.locator(".sidebar a[href],.sidebar button");
  await items.last().focus();
  await p.keyboard.press("Tab");
  assert.equal(
    await items.first().evaluate((e) => e === document.activeElement),
    true,
  );
  await p.keyboard.press("Shift+Tab");
  assert.equal(
    await items.last().evaluate((e) => e === document.activeElement),
    true,
  );
  await p.keyboard.press("Escape");
  assert.equal(
    await navButton.evaluate((e) => e === document.activeElement),
    true,
  );
  assert.equal(await p.locator(".sidebar").getAttribute("inert"), "");
  await p.waitForFunction(
    () =>
      document.querySelector(".sidebar").getBoundingClientRect().right <= 0.5,
  );
  await p.screenshot({
    path: join(artifacts, "design-fixed-mobile.png"),
    fullPage: true,
  });
  await navButton.click();
  await p.getByRole("button", { name: "语言", exact: true }).click();
  await p
    .getByRole("button", { name: "Close navigation", exact: true })
    .click();
  await p.waitForFunction(
    () =>
      document.querySelector(".sidebar").getBoundingClientRect().right <= 0.5,
  );
  await p.screenshot({
    path: join(artifacts, "design-fixed-mobile-en.png"),
    fullPage: true,
  });
  assert.equal(
    await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
    false,
  );
  await p.goto(base + "/shares");
  await p
    .getByRole("heading", { name: "No shares yet", exact: true })
    .waitFor();
  assert.equal(
    await p.locator(".empty img").getAttribute("alt"),
    "Magi, the Magic Cloud penguin",
  );
  await p.goto(base + "/trash");
  await p
    .getByRole("heading", { name: "All clean. Nothing here.", exact: true })
    .waitFor();
  await p.emulateMedia({ reducedMotion: "reduce" });
  assert(
    (await p
      .locator(".sidebar")
      .evaluate((e) => parseFloat(getComputedStyle(e).transitionDuration))) <
      0.01,
  );
  assert.deepEqual(errors, []);
  console.log(JSON.stringify(result, null, 2));
  writeFileSync(
    join(artifacts, "design-fixed-results.json"),
    JSON.stringify(result, null, 2),
  );
  console.log(
    "PASS real upload/dropzone/progress, categories, selection, list geometry, search, dialogs/menu keyboard/focus return, labeled settings, dark contrast, 320px drawer trap/Escape/inert, empty states, reduced motion",
  );
} finally {
  await c.close();
  await b.close();
}
