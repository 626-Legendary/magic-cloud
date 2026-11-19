// Read-only UI regression. Every API request is intercepted with fixture data.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
const artifacts = process.env.MC_TEST_ARTIFACTS;
if (artifacts) mkdirSync(artifacts, { recursive: true });
async function screenshot(page, name) {
  if (artifacts)
    await page.screenshot({
      path: join(artifacts, name + ".png"),
      fullPage: true,
    });
}
const base = process.env.MC_TEST_ORIGIN;
if (!base) throw new Error("Set MC_TEST_ORIGIN to the application origin.");
const browser = await chromium.connectOverCDP(
  process.env.MC_BROWSER_CDP || "http://127.0.0.1:19222",
);
const examples = [
  {
    id: "word",
    name: "Example.docx",
    kind: "file",
    size: 3000000,
    mtime: 1700000000,
    uploaded_at: 1700000010,
  },
  {
    id: "pdf",
    name: "Example.pdf",
    kind: "file",
    size: 1048576,
    mtime: 1700000000,
    uploaded_at: null,
  },
  {
    id: "invalid",
    name: "Invalid.md",
    kind: "file",
    size: 4,
    mtime: 1700000000,
  },
  { id: "empty", name: "Empty.txt", kind: "file", size: 0, mtime: 1700000000 },
  ...[
    "DOCX",
    "DOC",
    "XLSX",
    "PPTX",
    "PSD",
    "APK",
    "EXE",
    "DMG",
    "DEB",
    "APPIMAGE",
    "7Z",
    "ZIP",
    "RAR",
    "JSON",
    "JS",
    "TS",
    "PY",
    "unrecognized",
  ].map((extension) => ({
    id: extension,
    name: `Example.${extension}`,
    kind: "file",
    size: 1,
    mtime: 1700000000,
  })),
];
const nodes = { nodes: examples, breadcrumbs: [{ id: "root", name: "Root" }] };
const mediaNodes = [
  { id: "folder", name: "Folder", kind: "folder", size: 0, mtime: 1700000000 },
  ...["png", "mp3", "mp4"].map((extension) => ({
    id: extension,
    name: `Media.${extension}`,
    kind: "file",
    size: 1,
    mtime: 1700000000,
  })),
];
const fixtureSystem = {
  storage: { total: 1000000000, used: 1000 },
  system: { version: "1.0.1" },
};
const pendingUpload = {
  id: "fixture-upload",
  name: "Example.docx",
  parent: "root",
  size: 3000000,
  digest: "a".repeat(64),
  strategy: "ask",
  state: "pending",
  expires: 4102444800,
};
const errors = [];
const documentIcons = new Map();
async function iconSnapshot(artwork) {
  return artwork.evaluate((element) => ({
    tag: element.tagName.toLowerCase(),
    viewBox: element.getAttribute("viewBox"),
    drawing: element.innerHTML,
    color: getComputedStyle(element).color,
  }));
}
async function assertSameIcon(artwork, name) {
  const actual = await iconSnapshot(artwork);
  assert.equal(actual.tag, "svg");
  assert.deepEqual(
    actual,
    documentIcons.get(name),
    name + " must use the same document vector and color",
  );
}
async function context(mode = "owner", language = "zh", width = 1280) {
  const c = await browser.newContext({ viewport: { width, height: 900 } });
  await c.addInitScript(
    ({ language, width }) => {
      localStorage.setItem("mc-locale", language);
      localStorage.setItem("mc-theme", width === 320 ? "dark" : "light");
    },
    { language, width },
  );
  const requests = [];
  let shareRevoked = false;
  let shareExpires = null;
  const shareUpdates = [];
  await c.route("**/api/**", async (r) => {
    const path = new URL(r.request().url()).pathname;
    requests.push(path);
    let status = 200;
    let data = [];
    if (path === "/api/auth/session") {
      if (mode === "login") status = 401;
      data = { csrf: "fixture-only" };
    } else if (path === "/api/nodes")
      data = mode === "media" ? { ...nodes, nodes: mediaNodes } : nodes;
    else if (path.endsWith("/thumbnail")) {
      status = 415;
      data = { error: { code: "unsupported_preview" } };
    } else if (path === "/api/uploads") data = [pendingUpload];
    else if (path === "/api/uploads/fixture-upload")
      data = { upload: pendingUpload, chunk_size: 16777216, chunks: [] };
    else if (path === "/api/settings/backups") data = ["fixture-backup.db"];
    else if (path === "/api/trash")
      data = examples.map((node) => ({ ...node, deleted_at: 1700000000 }));
    else if (path === "/api/settings") data = fixtureSystem;
    else if (path === "/api/shares")
      data = [
        {
          id: "fixture-share",
          name: "Example.docx",
          revoked: shareRevoked,
          expires: shareExpires,
          url: "/s/fixture-word",
        },
      ];
    else if (path === "/api/shares/fixture-share") {
      if (r.request().method() === "PATCH") {
        shareExpires = r.request().postDataJSON().expires;
        shareUpdates.push(shareExpires);
      } else if (r.request().method() === "DELETE") shareRevoked = true;
      data = {};
    } else if (path.endsWith("/invalid/text")) {
      status = 400;
      data = { error: { code: "invalid_utf8" } };
    } else if (path.endsWith("/empty/text")) data = { text: "" };
    else if (path.startsWith("/api/public/")) {
      const selected = examples.find((n) => path.includes(n.id)) || examples[1];
      if (mode === "unavailable") {
        status = 404;
        data = { error: { code: "not_found" } };
      } else data = { root: selected, node: selected, nodes: [] };
    }
    await r.fulfill({
      status,
      contentType: "application/json",
      body: JSON.stringify(data),
    });
  });
  const p = await c.newPage();
  p.on("pageerror", (e) => errors.push(e.message));
  return { c, p, requests, shareUpdates };
}
try {
  for (const width of [1280, 320]) {
    const { c, p, requests, shareUpdates } = await context(
      "owner",
      "zh",
      width,
    );
    await p.goto(base + "/files/root");
    await p.locator(".file-item").first().waitFor();
    for (const example of examples) {
      const row = p.locator(".file-item").filter({
        has: p.getByRole("button", { name: example.name, exact: true }),
      });
      const badge = row.locator(".document-badge");
      const extension = example.name.split(".").at(-1);
      assert.equal(
        (await badge.textContent()).trim(),
        extension === "unrecognized" ? "FILE" : extension.toUpperCase(),
      );
      const paper = await row.locator(".document-artwork").boundingBox();
      const label = await badge.boundingBox();
      assert(
        label.x >= paper.x - 1 &&
          label.x + label.width <= paper.x + paper.width + 1,
      );
      assert(
        label.y >= paper.y - 1 &&
          label.y + label.height <= paper.y + paper.height + 1,
      );
      const snapshot = await iconSnapshot(row.locator(".document-artwork"));
      assert.equal(snapshot.tag, "svg");
      assert.equal(snapshot.viewBox, "0 0 80 88");
      if (documentIcons.has(example.name))
        assert.deepEqual(snapshot, documentIcons.get(example.name));
      else documentIcons.set(example.name, snapshot);
    }
    await p.locator(".transfer-title").click();
    await assertSameIcon(
      p.locator(".transfer-task-top .document-artwork"),
      "Example.docx",
    );
    await screenshot(p, `transfer-icon-${width}`);
    await p.locator(".transfer-title").click();
    await screenshot(p, `file-badges-grid-${width}`);
    await p.getByRole("button", { name: "切换视图 list", exact: true }).click();
    assert.equal(
      await p.locator(".document-artwork.is-compact").count(),
      examples.length,
    );
    for (const example of examples) {
      const row = p.locator(".file-item").filter({
        has: p.getByRole("button", { name: example.name, exact: true }),
      });
      await assertSameIcon(row.locator(".document-artwork"), example.name);
    }
    assert.deepEqual(
      documentIcons.get("Example.docx"),
      documentIcons.get("Example.DOCX"),
    );
    assert.equal(documentIcons.get("Example.docx").color, "rgb(37, 99, 235)");
    await screenshot(p, `file-badges-list-${width}`);
    await p.getByRole("button", { name: "切换视图 grid", exact: true }).click();
    assert.equal(
      await p
        .locator(".new-folder-button")
        .evaluate(
          (e) =>
            e.classList.contains("secondary") &&
            !e.classList.contains("primary"),
        ),
      true,
    );
    for (const item of examples.slice(0, 3)) {
      await p.getByRole("button", { name: item.name, exact: true }).click();
      const dialog = p.locator(".preview-dialog[open]");
      await dialog.getByText("该类型不支持在线预览", { exact: true }).waitFor();
      assert.equal(
        await dialog.locator("#preview-title").innerText(),
        item.name,
      );
      await assertSameIcon(dialog.locator(".document-artwork"), item.name);
      if (item.id === "word") await screenshot(p, `details-${width}`);
      assert.equal(await dialog.locator("iframe").count(), 0);
      for (const label of ["名称", "类型", "大小", "上传时间"])
        assert.equal(
          await dialog.locator("dt").filter({ hasText: label }).count(),
          1,
        );
      if (item.id === "pdf")
        assert.equal(
          await dialog.getByText("未记录", { exact: true }).count(),
          1,
        );
      await p.keyboard.press("Escape");
      await dialog.waitFor({ state: "hidden" });
    }
    assert.equal(
      requests.some((path) => /\/(word|pdf)\/text$/.test(path)),
      false,
    );
    await p.getByRole("button", { name: "Empty.txt", exact: true }).click();
    await p.locator(".preview-content[aria-busy=false] pre").waitFor();
    assert.equal(await p.locator(".preview-content pre").innerText(), "");
    await p.keyboard.press("Escape");
    const upload = p.locator(
      '.heading-actions button[aria-controls="upload-actions-dialog"]',
    );
    await upload.click();
    const uploadDialog = p.locator(".upload-choice-dialog[open]");
    await uploadDialog.waitFor();
    await screenshot(p, `upload-${width}`);
    const box = await uploadDialog.boundingBox();
    assert(Math.abs(box.x + box.width / 2 - width / 2) < 2);
    assert(Math.abs(box.y + box.height / 2 - 450) < 2);
    await p.keyboard.press("Escape");
    assert.equal(
      await upload.evaluate((e) => e === document.activeElement),
      true,
    );
    assert.equal(
      await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
    await p.goto(base + "/shares");
    const expiryCases = [
      ["5分钟", 300],
      ["1小时", 3600],
      ["6小时", 21600],
      ["12小时", 43200],
      ["7天", 604800],
      ["14天", 1209600],
      ["30天", 2592000],
      ["永久有效", null],
    ];
    for (const [label, seconds] of expiryCases) {
      await p.getByRole("button", { name: "修改有效期", exact: true }).click();
      const expiryDialog = p.getByRole("dialog", {
        name: "分享有效期",
        exact: true,
      });
      assert.equal(await expiryDialog.getByRole("radio").count(), 8);
      assert.equal(
        await expiryDialog
          .locator('input[type="text"],input[type="number"]')
          .count(),
        0,
      );
      await expiryDialog
        .getByRole("radio", { name: label, exact: true })
        .check();
      const before = Math.floor(Date.now() / 1000);
      const updated = p.waitForResponse(
        (response) =>
          response.request().method() === "PATCH" &&
          response.url().endsWith("/shares/fixture-share"),
      );
      await expiryDialog
        .getByRole("button", { name: "确认", exact: true })
        .click();
      await expiryDialog.waitFor({ state: "hidden" });
      await updated;
      const value = shareUpdates.at(-1);
      if (seconds === null) assert.equal(value, null);
      else
        assert(
          value >= before + seconds &&
            value <= Math.floor(Date.now() / 1000) + seconds,
        );
    }
    assert.equal(shareUpdates.includes(0), false);
    requests.length = 0;
    await p.getByRole("button", { name: "撤销", exact: true }).click();
    const revokeDialog = p.getByRole("dialog", {
      name: "撤销分享",
      exact: true,
    });
    await revokeDialog.getByText(/该操作会立即撤销分享/).waitFor();
    await screenshot(p, `revoke-${width}`);
    await revokeDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();
    assert.equal(requests.includes("/api/shares/fixture-share"), false);
    await p.getByRole("button", { name: "撤销", exact: true }).click();
    await revokeDialog
      .getByRole("button", { name: "撤销分享", exact: true })
      .click();
    await p.getByRole("button", { name: "删除记录", exact: true }).waitFor();
    assert.equal(
      requests.filter((path) => path === "/api/shares/fixture-share").length,
      1,
    );
    await p.goto(base + "/recent");
    await p.locator(".document-artwork").first().waitFor();
    for (const example of examples)
      await assertSameIcon(
        p
          .locator(".file-item")
          .filter({
            has: p.getByRole("button", { name: example.name, exact: true }),
          })
          .locator(".document-artwork"),
        example.name,
      );
    await p.goto(base + "/trash");
    await p.locator(".data-row .document-badge").first().waitFor();
    assert.equal(
      await p.locator(".data-row .document-badge").count(),
      examples.length,
    );
    for (const example of examples)
      await assertSameIcon(
        p
          .locator(".data-row")
          .filter({
            has: p.locator("strong").getByText(example.name, { exact: true }),
          })
          .locator(".document-artwork"),
        example.name,
      );
    await p.goto(base + "/settings");
    await p.locator(".about-panel").waitFor();
    const backup = p
      .locator(".data-row")
      .filter({ has: p.locator(".backup-name") });
    await assertSameIcon(
      backup.locator(".document-artwork"),
      "Example.unrecognized",
    );
    assert.match(await p.locator(".about-panel").innerText(), /v1\.0\.1/);
    assert.equal(await p.locator(".about-panel li").count(), 5);
    assert.match(
      await p.locator(".about-panel").innerText(),
      /支持本地及外置磁盘存储/,
    );
    await p.locator("#language-preference").selectOption("en");
    assert.match(
      await p.locator(".about-panel").innerText(),
      /Internal directories and external data disks/,
    );
    assert.equal(
      await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
    await screenshot(p, `about-${width}`);
    await c.close();
  }
  for (const width of [1280, 320]) {
    const { c, p } = await context("media", "zh", width);
    await p.goto(base + "/files/root");
    await p.locator(".folder-artwork").waitFor();
    assert.equal(
      await p.locator(".folder-artwork").getAttribute("viewBox"),
      "0 0 96 80",
    );
    assert.equal(
      await p
        .locator(".audio-artwork")
        .evaluate((e) => getComputedStyle(e).color),
      "rgb(255, 255, 255)",
    );
    assert.equal(await p.locator(".file-item.image .media-artwork").count(), 1);
    assert.equal(await p.locator(".file-item.video .media-artwork").count(), 1);
    await p.getByRole("button", { name: "切换视图 list", exact: true }).click();
    const compactFolder = await p
      .locator(".folder-artwork.is-compact")
      .boundingBox();
    assert.equal(compactFolder.width, 28);
    assert.equal(compactFolder.height, 28);
    for (const icon of ["image", "music", "video"])
      assert.equal(
        await p.locator(`.file-symbol svg[class*="lucide-${icon}"]`).count(),
        1,
      );
    assert.equal(
      await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
    await c.close();
  }
  for (const width of [1280, 320]) {
    const { c, p, requests } = await context("visitor", "zh", width);
    await p.goto(base + "/s/fixture-pdf");
    await p.locator(".public-download-card").waitFor();
    const card = p.locator(".public-download-card");
    await p.getByText("安全加密分享", { exact: true }).waitFor();
    assert.equal(
      await p
        .locator('.public-header svg[class*="lucide-lock-keyhole"]')
        .count(),
      1,
    );
    assert.equal(
      await p.getByText("下载到设备后即可打开。", { exact: true }).count(),
      0,
    );
    assert.equal(
      await p
        .getByText("可下载原文件，在你的设备上打开。", { exact: true })
        .count(),
      0,
    );
    assert.equal(
      await card.locator("img, video, audio, iframe, .media-thumbnail").count(),
      0,
    );
    assert.equal(
      await card.getByRole("button", { name: "下载文件", exact: true }).count(),
      1,
    );
    assert.equal(
      await card.locator(".shared-file-name").textContent(),
      "Example.pdf",
    );
    assert.equal(
      await card.locator(".shared-download-file .file-name").textContent(),
      "Example.pdf",
    );
    assert.equal((await card.textContent()).split("Example.pdf").length - 1, 2);
    assert.deepEqual(
      await card.locator(".document-badge text").allTextContents(),
      ["PDF", "PDF"],
    );
    assert.equal(
      requests.some((path) => /\/(thumbnail|text|download)$/.test(path)),
      false,
    );
    await screenshot(p, `visitor-${width}`);
    assert.equal(
      await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
    for (const artwork of await card.locator(".document-artwork").all())
      await assertSameIcon(artwork, "Example.pdf");
    await p.goto(base + "/s/fixture-word");
    await p.locator(".public-download-card").waitFor();
    for (const artwork of await p
      .locator(".public-download-card .document-artwork")
      .all())
      await assertSameIcon(artwork, "Example.docx");
    await screenshot(p, `word-share-${width}`);
    await c.close();
  }
  for (const width of [1280, 320]) {
    const { c, p } = await context("login", "en", width);
    await p.goto(base);
    await p
      .getByRole("heading", { name: "Welcome home.", exact: true })
      .waitFor();
    await p
      .getByText("Your digital nest.", {
        exact: true,
      })
      .waitFor();
    await p
      .getByText("Your files, under your control", { exact: true })
      .waitFor();
    await screenshot(p, `login-${width}`);
    assert.deepEqual(
      await p.locator(".login-features li span").allTextContents(),
      ["Privacy", "Storage", "Big Files", "Downloads", "Sync"],
    );
    const positions = await p
      .locator(".login-features li")
      .evaluateAll((es) =>
        es.map((e) => ({ x: e.offsetLeft, y: e.offsetTop, w: e.clientWidth })),
      );
    assert.equal(positions[0].y, positions[1].y);
    assert.equal(positions[1].y, positions[2].y);
    assert.equal(positions[3].y, positions[4].y);
    assert.equal(
      await p.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
    await c.close();
  }

  const unavailable = await context("unavailable", "zh", 320);
  await unavailable.p.goto(base + "/s/fixture-unavailable");
  await unavailable.p
    .getByText("链接可能已过期或被撤销，分享的文件也可能已删除。", {
      exact: true,
    })
    .waitFor();
  assert.equal(
    await unavailable.p.getByText(/请联系分享者获取新的链接/).count(),
    0,
  );
  await unavailable.c.close();
  assert.deepEqual(errors, []);
  console.log(
    "PASS identical document SVG/color in grid/list/recent/trash/details/share/transfers/backups and case-insensitive badges, bilingual about v1.0.1, preview/details, centered upload, download-only visitor, expiry/revoke and responsive English login",
  );
} finally {
  await browser.close();
}
