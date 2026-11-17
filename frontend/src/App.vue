<script setup lang="ts">
import { copyText } from "./browser";
import heroImage from "./assets/Hero.png";
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  Cloud,
  Folder,
  File,
  FileText,
  Image,
  Music,
  Film,
  Search,
  Upload as UploadIcon,
  FolderPlus,
  FolderUp,
  Languages,
  Link2Off,
  WifiOff,
  Grid2X2,
  List,
  ChevronRight,
  ChevronDown,
  ArrowDownToLine,
  MoreHorizontal,
  Settings,
  Clock,
  Link,
  Trash2,
  LogOut,
  Sun,
  Moon,
  Monitor,
  ArrowUpDown,
  X,
  Pause,
  Play,
  RotateCcw,
  Check,
  AlertCircle,
  Menu,
  HardDrive,
  ShieldCheck,
  ArrowLeft,
  Copy,
  ExternalLink,
  RefreshCw,
} from "lucide-vue-next";
import { api, ApiError, setCsrf, bytes, date, type Node } from "./api";
import { locale, theme, t, applyPreferences } from "./i18n";
import {
  transfer,
  enqueue,
  pause,
  resume,
  recoverTasks,
  reselect,
  cancelTask,
  stopAll,
  clearFinished,
  MAX_FILE,
  type Task,
} from "./transfer";
const route = useRoute(),
  router = useRouter(),
  locationOrigin = location.origin;
const authenticated = ref(false),
  checking = ref(true),
  password = ref(""),
  loginBusy = ref(false),
  loading = ref(false),
  error = ref(""),
  toast = ref(""),
  sidebar = ref(false);
const nodes = ref<Node[]>([]),
  breadcrumbs = ref<Node[]>([]),
  shareRows = ref<any[]>([]),
  trashRows = ref<any[]>([]),
  logs = ref<any[]>([]),
  backupRows = ref<string[]>([]),
  system = ref<any>(null),
  publicData = ref<any>(null),
  publicError = ref<"unavailable" | "network" | null>(null);
const search = ref(""),
  sort = ref("name"),
  desc = ref(false),
  grid = ref(localStorage.getItem("mc-grid") !== "false"),
  menu = ref(""),
  drop = ref(false),
  fileInput = ref<HTMLInputElement>(),
  folderInput = ref<HTMLInputElement>(),
  recoverInput = ref<HTMLInputElement>(),
  recoverTask = ref<Task>();
const view = computed(() =>
  route.path.startsWith("/s/") ? "public" : route.path.split("/")[1] || "files",
);
const parent = computed(() => String(route.params.id || "root"));
const nav = [
  { key: "files", icon: Cloud, path: "/files/root" },
  { key: "recent", icon: Clock, path: "/recent" },
  { key: "shares", icon: Link, path: "/shares" },
  { key: "trash", icon: Trash2, path: "/trash" },
  { key: "settings", icon: Settings, path: "/settings" },
];
const title = computed(() =>
  view.value === "files"
    ? breadcrumbs.value.at(-1)?.name || t("home")
    : t(view.value),
);
const active = computed(
  () =>
    transfer.tasks.filter((x) => !["complete", "cancelled"].includes(x.status))
      .length,
);
const modal = ref<HTMLDialogElement>(),
  modalTitle = ref(""),
  modalHint = ref(""),
  modalValue = ref(""),
  modalInput = ref(false),
  modalChoices = ref<{ label: string; value: string; danger?: boolean }[]>([]);
let resolveModal: ((v: string | null) => void) | undefined;
async function ask(
  title: string,
  options: {
    input?: string;
    hint?: string;
    choices?: { label: string; value: string; danger?: boolean }[];
  } = {},
): Promise<string | null> {
  modalTitle.value = title;
  modalHint.value = options.hint || "";
  modalInput.value = options.input !== undefined;
  modalValue.value = options.input || "";
  modalChoices.value = options.choices || [
    { label: t("confirm"), value: "yes" },
  ];
  await nextTick();
  modal.value?.showModal();
  return new Promise((resolve) => {
    resolveModal = resolve;
  });
}
function answer(value: string | null) {
  modal.value?.close();
  resolveModal?.(value);
  resolveModal = undefined;
}
function fail(e: unknown) {
  error.value = t(
    e instanceof ApiError ? e.code : e instanceof Error ? e.message : "error",
  );
  if (e instanceof ApiError && e.status === 401) {
    authenticated.value = false;
    stopAll();
  }
}
let toastTimer: ReturnType<typeof setTimeout>;
function notify(value: string) {
  toast.value = value;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.value = ""), 4500);
}
let loadVersion = 0;
async function load() {
  const version = ++loadVersion;
  error.value = "";
  loading.value = true;
  menu.value = "";
  sidebar.value = false;
  try {
    if (view.value === "public") {
      publicData.value = null;
      publicError.value = null;
      const data = await api(
        `/public/${route.params.token}${route.query.node ? "?node=" + encodeURIComponent(String(route.query.node)) : ""}`,
      );
      if (version === loadVersion) publicData.value = data;
      return;
    }
    if (!authenticated.value) return;
    if (view.value === "files" || view.value === "recent") {
      const params = new URLSearchParams({
        parent: parent.value,
        sort: sort.value,
        desc: String(view.value === "recent" || desc.value),
      });
      if (search.value) params.set("q", search.value);
      if (view.value === "recent") {
        params.set("recent", "true");
        params.set("sort", "mtime");
      }
      const data = await api("/nodes?" + params);
      if (version === loadVersion) {
        nodes.value = data.nodes;
        breadcrumbs.value = data.breadcrumbs;
      }
    } else if (view.value === "shares") shareRows.value = await api("/shares");
    else if (view.value === "trash") trashRows.value = await api("/trash");
    else if (view.value === "settings") {
      const [s, l, b] = await Promise.all([
        api("/settings"),
        api("/settings/audit"),
        api<string[]>("/settings/backups"),
      ]);
      system.value = s;
      logs.value = l;
      backupRows.value = b;
    }
  } catch (e) {
    if (version === loadVersion) {
      if (view.value === "public") {
        publicData.value = null;
        publicError.value =
          e instanceof ApiError &&
          (e.status === 404 ||
            e.status === 410 ||
            e.code === "file_identity_changed")
            ? "unavailable"
            : "network";
      } else fail(e);
    }
  } finally {
    if (version === loadVersion) loading.value = false;
  }
}
async function signIn() {
  loginBusy.value = true;
  error.value = "";
  try {
    const data = await api("/auth/login", "POST", { password: password.value });
    password.value = "";
    setCsrf(data.csrf);
    authenticated.value = true;
    await recoverTasks();
    await load();
  } catch (e) {
    fail(e);
  } finally {
    loginBusy.value = false;
  }
}
async function signOut() {
  try {
    await api("/auth/logout", "POST", {});
    stopAll();
    authenticated.value = false;
    setCsrf("");
  } catch (e) {
    fail(e);
  }
}
async function refreshStorage() {
  try {
    system.value = await api("/settings");
  } catch {
    /* Main view reports availability failures. */
  }
}
function icon(n: Node) {
  if (n.kind === "folder") return Folder;
  const ext = n.name.split(".").pop()?.toLowerCase() || "";
  if (/^(png|jpe?g|gif|webp|avif|bmp|svg)$/.test(ext)) return Image;
  if (/^(mp4|mkv|mov|webm|avi|m4v)$/.test(ext)) return Film;
  if (/^(mp3|wav|ogg|flac|m4a|aac)$/.test(ext)) return Music;
  if (/^(pdf|txt|md|rs|js|ts|json|csv|py|html|css)$/.test(ext)) return FileText;
  return File;
}
function kindClass(n: Node) {
  return n.kind === "folder"
    ? "folder"
    : icon(n) === Image
      ? "image"
      : icon(n) === Film
        ? "video"
        : icon(n) === Music
          ? "audio"
          : n.name.endsWith(".pdf")
            ? "pdf"
            : "document";
}
async function conflict(fn: (strategy: string) => Promise<unknown>) {
  try {
    await fn("ask");
  } catch (e) {
    if (e instanceof ApiError && e.code === "name_conflict") {
      const strategy = await ask(t("conflict"), {
        choices: [
          { label: t("overwrite"), value: "overwrite", danger: true },
          { label: t("autoRename"), value: "rename" },
        ],
      });
      if (strategy) await fn(strategy);
      else return false;
    } else throw e;
  }
  return true;
}
async function makeFolder() {
  const name = await ask(t("newFolder"), { input: "", hint: t("folderName") });
  if (!name) return;
  try {
    await api("/nodes", "POST", { parent: parent.value, name });
    await load();
  } catch (e) {
    fail(e);
  }
}
async function renameNode(n: Node) {
  menu.value = "";
  const name = await ask(t("rename"), { input: n.name, hint: t("newName") });
  if (!name || name === n.name) return;
  try {
    await conflict((strategy) =>
      api(`/nodes/${n.id}`, "PATCH", { name, parent: n.parent, strategy }),
    );
    await load();
  } catch (e) {
    fail(e);
  }
}
async function deleteNode(n: Node) {
  menu.value = "";
  if (
    !(await ask(t("confirmDelete"), {
      hint: n.name,
      choices: [{ label: t("delete"), value: "yes", danger: true }],
    }))
  )
    return;
  try {
    await api(`/nodes/${n.id}`, "DELETE");
    await load();
    await refreshStorage();
  } catch (e) {
    fail(e);
  }
}
const moveDialog = ref<HTMLDialogElement>(),
  moving = ref<Node>(),
  moveParent = ref("root"),
  moveNodes = ref<Node[]>([]),
  moveCrumbs = ref<Node[]>([]);
async function loadMove(id: string) {
  try {
    const data = await api("/nodes?parent=" + id);
    moveParent.value = id;
    moveNodes.value = data.nodes.filter(
      (x: Node) => x.kind === "folder" && x.id !== moving.value?.id,
    );
    moveCrumbs.value = data.breadcrumbs;
  } catch (e) {
    fail(e);
  }
}
async function startMove(n: Node) {
  menu.value = "";
  moving.value = n;
  await loadMove("root");
  moveDialog.value?.showModal();
}
async function finishMove() {
  if (!moving.value) return;
  try {
    const n = moving.value;
    moveDialog.value?.close();
    await conflict((strategy) =>
      api(`/nodes/${n.id}`, "PATCH", {
        parent: moveParent.value,
        name: n.name,
        strategy,
      }),
    );
    await load();
  } catch (e) {
    fail(e);
  }
}
async function restoreNode(n: any) {
  try {
    await conflict((strategy) =>
      api(`/trash/${n.id}/restore`, "POST", { strategy }),
    );
    await load();
  } catch (e) {
    fail(e);
  }
}
async function purgeNode(n: any) {
  if (
    !(await ask(t("confirmPurge"), {
      hint: n.name,
      choices: [{ label: t("purge"), value: "yes", danger: true }],
    }))
  )
    return;
  try {
    await api(`/trash/${n.id}`, "DELETE");
    await load();
    await refreshStorage();
  } catch (e) {
    fail(e);
  }
}
async function emptyTrash() {
  if (
    !(await ask(t("confirmEmpty"), {
      choices: [{ label: t("emptyTrash"), value: "yes", danger: true }],
    }))
  )
    return;
  try {
    await api("/trash", "DELETE");
    await load();
    await refreshStorage();
  } catch (e) {
    fail(e);
  }
}
const shareLink = ref(""),
  shareDialog = ref<HTMLDialogElement>();
async function days(): Promise<number | null | undefined> {
  const value = await ask(t("expires"), { input: "", hint: t("expiryPrompt") });
  if (value === null) return undefined;
  if (value === "") return null;
  const num = Number(value);
  if (!Number.isFinite(num) || num <= 0) {
    throw new Error("invalid_expiry");
  }
  return Math.floor(Date.now() / 1000 + num * 86400);
}
async function shareNode(n: Node) {
  menu.value = "";
  try {
    const expires = await days();
    if (expires === undefined) return;
    const data = await api("/shares", "POST", { root: n.id, expires });
    shareLink.value = location.origin + data.url;
    shareDialog.value?.showModal();
  } catch (e) {
    fail(e);
  }
}
async function editExpiry(s: any) {
  try {
    const expires = await days();
    if (expires === undefined) return;
    await api("/shares/" + s.id, "PATCH", { expires });
    await load();
  } catch (e) {
    fail(e);
  }
}
async function revoke(s: any) {
  if (!(await ask(t("revoke"), { hint: s.name }))) return;
  try {
    await api("/shares/" + s.id, "DELETE");
    await load();
  } catch (e) {
    fail(e);
  }
}
async function copy(value: string) {
  try {
    await copyText(value);
    notify(t("copied"));
  } catch (e) {
    fail(e);
  }
}
async function createBackup() {
  try {
    await api("/settings/backups", "POST", {});
    backupRows.value = await api("/settings/backups");
  } catch (e) {
    fail(e);
  }
}
const preview = ref<Node>(),
  previewDialog = ref<HTMLDialogElement>(),
  previewText = ref(""),
  previewError = ref("");
const mediaType = computed(() => (preview.value ? icon(preview.value) : null));
const previewUrl = computed(() =>
  preview.value ? `/api/nodes/${preview.value.id}/download?inline=true` : "",
);
async function openNode(n: Node) {
  menu.value = "";
  if (n.kind === "folder") {
    search.value = "";
    router.push("/files/" + n.id);
    return;
  }
  preview.value = n;
  previewText.value = "";
  previewError.value = "";
  previewDialog.value?.showModal();
  if (
    ![Image, Film, Music].includes(icon(n)) &&
    !n.name.toLowerCase().endsWith(".pdf")
  ) {
    try {
      previewText.value = (await api(`/nodes/${n.id}/text`)).text;
    } catch (e) {
      previewError.value = t(e instanceof ApiError ? e.code : "noPreview");
    }
  }
}
function closePreview() {
  preview.value = undefined;
  previewDialog.value?.close();
}
function download(n: Node) {
  window.location.assign(`/api/nodes/${n.id}/download`);
}
function uploadSelected(event: Event) {
  const input = event.target as HTMLInputElement;
  void uploadFiles(Array.from(input.files || []));
  input.value = "";
}
async function uploadFiles(files: File[]) {
  try {
    if (files.some((f) => f.size > MAX_FILE)) throw new Error("maxSize");
    const dirs = new Map<string, string>([["", parent.value]]);
    for (const file of files) {
      const relative = file.webkitRelativePath || file.name;
      const parts = relative.split("/");
      if (parts.some((p) => !p || p === ".." || p === "." || p.includes("\\")))
        throw new Error("invalid_name");
      let path = "";
      let pid = parent.value;
      for (const part of parts.slice(0, -1)) {
        const next = path ? path + "/" + part : part;
        if (!dirs.has(next)) {
          let folder: Node;
          try {
            folder = await api("/nodes", "POST", { parent: pid, name: part });
          } catch (e) {
            if (!(e instanceof ApiError && e.code === "name_conflict")) throw e;
            const data = await api("/nodes?parent=" + pid);
            folder = data.nodes.find(
              (n: Node) => n.name === part && n.kind === "folder",
            );
            if (!folder) throw e;
          }
          dirs.set(next, folder.id);
        }
        pid = dirs.get(next)!;
        path = next;
      }
      enqueue(file, pid);
    }
    await load();
  } catch (e) {
    fail(e);
  }
}
async function dropped(event: DragEvent) {
  drop.value = false;
  if (view.value !== "files") return;
  const entries = Array.from(event.dataTransfer?.items || [])
    .map((x) => (x as any).webkitGetAsEntry?.())
    .filter(Boolean);
  if (!entries.length) {
    await uploadFiles(Array.from(event.dataTransfer?.files || []));
    return;
  }
  const files: File[] = [];
  async function walk(entry: any, path = "") {
    if (entry.isFile) {
      const f: File = await new Promise((res, rej) => entry.file(res, rej));
      Object.defineProperty(f, "webkitRelativePath", { value: path + f.name });
      files.push(f);
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      for (;;) {
        const batch: any[] = await new Promise((res, rej) =>
          reader.readEntries(res, rej),
        );
        if (!batch.length) break;
        for (const child of batch) await walk(child, path + entry.name + "/");
      }
    }
  }
  try {
    for (const entry of entries) await walk(entry);
    await uploadFiles(files);
  } catch (e) {
    fail(e);
  }
}
function chooseOriginal(task: Task) {
  recoverTask.value = task;
  recoverInput.value?.click();
}
function chosenOriginal(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  try {
    if (file && recoverTask.value) reselect(recoverTask.value, file);
  } catch (e) {
    fail(e);
  }
  input.value = "";
}
async function taskRetry(task: Task) {
  if (task.error === "name_conflict") {
    const strategy = await ask(t("conflict"), {
      choices: [
        { label: t("overwrite"), value: "overwrite", danger: true },
        { label: t("autoRename"), value: "rename" },
      ],
    });
    if (strategy) {
      if (task.id) {
        /* Completion conflict must be resolved on the persisted task. */ try {
          await api(`/uploads/${task.id}/strategy`, "PATCH", { strategy });
        } catch (e) {
          fail(e);
          return;
        }
      }
      resume(task, strategy);
    }
  } else resume(task);
}
async function cancelTransfer(task: Task) {
  try {
    await cancelTask(task);
  } catch (e) {
    task.status = "failed";
    task.error = e instanceof ApiError ? e.code : "request_failed";
    fail(e);
  }
}
function publicOpen(n: Node) {
  if (n.kind === "folder")
    router.push({ path: route.path, query: { node: n.id } });
  else
    window.location.assign(
      `/api/public/${route.params.token}/nodes/${n.id}/download`,
    );
}
let searchTimer: ReturnType<typeof setTimeout>;
watch(search, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void load(), 250);
});
watch([sort, desc], () => void load());
watch(grid, () => localStorage.setItem("mc-grid", String(grid.value)));
watch([locale, theme], applyPreferences);
watch(
  () => route.fullPath,
  () => {
    search.value = "";
    void load();
  },
);
const systemTheme = matchMedia("(prefers-color-scheme: dark)");
const changed = () => {
  notify(t("uploadSuccess"));
  void load();
  void refreshStorage();
};
onMounted(async () => {
  applyPreferences();
  systemTheme.addEventListener("change", applyPreferences);
  window.addEventListener("mc-files-changed", changed);
  if (view.value === "public") {
    checking.value = false;
    await load();
    return;
  }
  try {
    const data = await api("/auth/session");
    setCsrf(data.csrf);
    authenticated.value = true;
    await recoverTasks();
    await refreshStorage();
  } catch (e) {
    if (!(e instanceof ApiError && e.status === 401)) fail(e);
  } finally {
    checking.value = false;
    await load();
  }
});
onUnmounted(() => {
  systemTheme.removeEventListener("change", applyPreferences);
  window.removeEventListener("mc-files-changed", changed);
  clearTimeout(searchTimer);
  clearTimeout(toastTimer);
});
</script>

<template>
  <div v-if="checking" class="boot">
    <img src="/Logo.png" alt="Magic Cloud" /><span class="spinner"></span>
  </div>
  <main v-else-if="view === 'public'" class="public-page">
    <header class="public-header">
      <a href="/"
        ><img src="/Logo.png" alt="" /><b>{{ t("brand") }}</b></a
      ><span><ShieldCheck :size="16" />{{ t("readOnly") }}</span
      ><button
        :aria-label="t('language')"
        :title="t('language')"
        @click="locale = locale === 'zh' ? 'en' : 'zh'"
      >
        <Languages :size="18" />
      </button>
    </header>
    <section
      v-if="publicError"
      class="public-card share-unavailable"
      role="alert"
    >
      <div class="share-error-icon">
        <component
          :is="publicError === 'unavailable' ? Link2Off : WifiOff"
          :size="34"
        />
      </div>
      <h1>
        {{
          t(
            publicError === "unavailable"
              ? "shareUnavailableTitle"
              : "networkErrorTitle",
          )
        }}
      </h1>
      <p>
        {{
          t(
            publicError === "unavailable"
              ? "shareUnavailableHint"
              : "network_error",
          )
        }}
      </p>
      <div class="share-error-actions">
        <button v-if="route.query.node" @click="router.replace(route.path)">
          <ArrowLeft :size="16" />{{ t("shareHome") }}
        </button>
        <button @click="load">
          <RefreshCw :size="16" />{{ t("checkAgain") }}
        </button>
        <RouterLink class="button primary" to="/">{{ t("goHome") }}</RouterLink>
      </div>
    </section>
    <section v-if="publicData" class="public-card">
      <span class="eyebrow">{{ t("readOnly") }}</span>
      <h1>{{ publicData.root.name || t("brand") }}</h1>
      <div class="toolbar">
        <button v-if="route.query.node" @click="router.push(route.path)">
          <ArrowLeft :size="16" />{{ t("back") }}</button
        ><span>{{ publicData.node.name }}</span>
      </div>
      <template v-if="publicData.node.kind === 'folder'"
        ><button
          v-for="n in publicData.nodes"
          :key="n.id"
          class="public-file"
          @click="publicOpen(n)"
        >
          <component :is="icon(n)" :class="kindClass(n)" :size="26" /><strong>{{
            n.name
          }}</strong
          ><small>{{ n.kind === "folder" ? t("folder") : bytes(n.size) }}</small
          ><component
            :is="n.kind === 'folder' ? ChevronRight : ArrowDownToLine"
            :size="18"
          />
        </button>
        <p v-if="!publicData.nodes.length" class="muted">
          {{ t("emptyList") }}
        </p></template
      >
      <div v-else class="public-single">
        <component
          :is="icon(publicData.node)"
          :size="70"
          :class="kindClass(publicData.node)"
        />
        <h2>{{ publicData.node.name }}</h2>
        <p class="muted">
          {{ bytes(publicData.node.size) }} · {{ date(publicData.node.mtime) }}
        </p>
        <button class="primary" @click="publicOpen(publicData.node)">
          <ArrowDownToLine :size="18" />{{ t("download") }}
        </button>
      </div>
    </section>
    <div v-else-if="loading" class="empty"><span class="spinner"></span></div>
  </main>
  <main v-else-if="!authenticated" class="login-page">
    <div class="login-controls">
      <button
        :aria-label="t('language')"
        :title="t('language')"
        @click="locale = locale === 'zh' ? 'en' : 'zh'"
      >
        <Languages :size="18" /></button
      ><button
        :aria-label="t('appearance')"
        @click="theme = theme === 'dark' ? 'light' : 'dark'"
      >
        <component :is="theme === 'dark' ? Sun : Moon" :size="18" />
      </button>
    </div>
    <section class="login-brand">
      <img
        class="hero-image"
        :src="heroImage"
        alt=""
        aria-hidden="true"
        width="1122"
        height="1402"
        fetchpriority="high"
        decoding="async"
      />
      <div class="hero-shade" aria-hidden="true"></div>
      <span class="brand-label"
        ><img src="/Logo.png" alt="" width="42" height="42" />{{
          t("brand")
        }}</span
      >
      <div class="hero-copy">
        <h1>{{ t("tagline") }}</h1>
        <p>{{ t("heroDescription") }}</p>
        <p class="hero-support">{{ t("heroSupport") }}</p>
      </div>
      <div class="privacy-pill">
        <ShieldCheck :size="16" />{{ t("secure") }}
      </div>
    </section>
    <section class="login-form-wrap">
      <form class="login-form" @submit.prevent="signIn">
        <span class="eyebrow">YOUR PERSONAL CLOUD</span>
        <h2>{{ t("login") }}</h2>
        <p class="muted">{{ t("loginHint") }}</p>
        <label for="password">{{ t("password") }}</label><input
          id="password"
          v-model="password"
          type="password"
          autocomplete="current-password"
          required
          autofocus
          :placeholder="t('password')"
        />
        <div v-if="error" role="alert" class="notice error">
          <AlertCircle :size="16" />{{ error }}
        </div>
        <button class="primary" :disabled="loginBusy || !password">
          <span v-if="loginBusy" class="spinner"></span
          ><span v-else>{{ t("login") }}</span
          ><ChevronRight :size="18" />
        </button>
        <p class="login-privacy">
          <ShieldCheck :size="16" />{{ t("privacy") }}
        </p>
      </form>
      <small class="login-footer">{{ t("productLead") }}</small>
    </section>
  </main>
  <div v-else class="app-shell" @click="menu = ''">
    <div v-if="sidebar" class="sidebar-backdrop" @click="sidebar = false"></div>
    <aside class="sidebar" :class="{ open: sidebar }">
      <a class="brand" href="/" @click.prevent="router.push('/files/root')"
        ><img src="/Logo.png" alt="" /><span
          >{{ t("brand") }}<small>YOUR PERSONAL CLOUD</small></span
        ></a
      ><span class="nav-label">{{
        locale === "zh" ? "我的空间" : "WORKSPACE"
      }}</span>
      <nav>
        <RouterLink
          v-for="item in nav"
          :key="item.key"
          :to="item.path"
          :class="{ active: view === item.key }"
          ><component :is="item.icon" :size="20" /><span>{{
            t(item.key)
          }}</span></RouterLink
        >
      </nav>
      <div class="sidebar-bottom">
        <section v-if="system" class="storage-mini">
          <div>
            <HardDrive :size="17" /><span>{{ t("storage") }}</span
            ><span class="online-dot"></span>
          </div>
          <div class="meter">
            <i
              :style="{
                width:
                  Math.min(
                    100,
                    (system.storage.used / system.storage.total) * 100,
                  ) + '%',
              }"
            ></i>
          </div>
          <small
            >{{ bytes(system.storage.used) }} /
            {{ bytes(system.storage.total) }}</small
          >
          <p>{{ t("storageHint") }}</p>
        </section>
        <div class="sidebar-actions">
          <button
            :aria-label="t('appearance')"
            @click="
              theme =
                theme === 'dark'
                  ? 'light'
                  : theme === 'light'
                    ? 'system'
                    : 'dark'
            "
          >
            <component
              :is="theme === 'dark' ? Moon : theme === 'light' ? Sun : Monitor"
              :size="18"
            /></button
          ><button
            :aria-label="t('language')"
            :title="t('language')"
            @click="locale = locale === 'zh' ? 'en' : 'zh'"
          >
            <Languages :size="18" /></button
          ><button :aria-label="t('logout')" @click="signOut">
            <LogOut :size="18" />
          </button>
        </div>
      </div>
    </aside>
    <div class="workspace">
      <header class="topbar">
        <button
          class="mobile-only icon-button"
          :aria-label="t('menu')"
          @click="sidebar = true"
        >
          <Menu :size="22" />
        </button>
        <div class="search" v-if="['files', 'recent'].includes(view)">
          <Search :size="19" /><input
            v-model="search"
            :placeholder="t('search')"
            :aria-label="t('search')"
          /><kbd>/</kbd>
        </div>
        <span v-else class="topbar-label">{{ t("brand") }}</span>
        <div class="topbar-status">
          <span class="online-dot"></span><span>{{ t("secure") }}</span>
          <div class="avatar"><Cloud :size="21" /></div>
        </div>
      </header>
      <div
        class="content"
        @dragover.prevent="view === 'files' && (drop = true)"
        @dragleave.self="drop = false"
        @drop.prevent="dropped"
      >
        <div v-if="drop && view === 'files'" class="drop-overlay">
          <UploadIcon :size="48" />
          <h2>{{ t("drop") }}</h2>
        </div>
        <div class="page-heading">
          <div>
            <div v-if="view === 'files'" class="breadcrumbs">
              <button
                v-for="(b, i) in breadcrumbs"
                :key="b.id"
                @click="router.push('/files/' + b.id)"
              >
                <ChevronRight v-if="i" :size="13" />{{
                  b.id === "root" ? t("home") : b.name
                }}
              </button>
            </div>
            <span v-else class="eyebrow">YOUR PERSONAL CLOUD</span>
            <h1>{{ title }}</h1>
            <p v-if="view === 'files'" class="muted">
              {{ nodes.length }} {{ locale === "zh" ? "个项目" : "items" }}
              <span class="dot-separator">·</span> {{ t("welcome") }}
            </p>
            <p v-if="view === 'trash'" class="muted">{{ t("trashHint") }}</p>
          </div>
          <div class="heading-actions">
            <button v-if="view === 'files'" @click="makeFolder">
              <FolderPlus :size="18" /><span>{{ t("newFolder") }}</span></button
            ><button
              v-if="view === 'files'"
              class="primary"
              @click="fileInput?.click()"
            >
              <UploadIcon :size="18" />{{ t("upload") }}</button
            ><button
              v-if="view === 'files'"
              class="folder-upload-button"
              @click="folderInput?.click()"
            >
              <FolderUp :size="18" />{{ t("uploadFolder") }}</button
            ><button
              v-if="view === 'trash' && trashRows.length"
              class="danger"
              @click="emptyTrash"
            >
              <Trash2 :size="17" />{{ t("emptyTrash") }}
            </button>
          </div>
        </div>
        <div v-if="error" role="alert" class="notice error">
          <AlertCircle :size="18" /><span>{{ error }}</span
          ><button :aria-label="t('close')" @click="error = ''">
            <X :size="16" />
          </button>
        </div>
        <template v-if="['files', 'recent'].includes(view)"
          ><div class="toolbar">
            <div class="filter-label">
              <Folder :size="16" />{{
                search ? t("search").replace("…", "") : t("home")
              }}
            </div>
            <div class="toolbar-right">
              <select v-model="sort" :aria-label="t('type')">
                <option value="name">{{ t("name") }}</option>
                <option value="size">{{ t("size") }}</option>
                <option value="type">{{ t("type") }}</option>
                <option value="mtime">{{ t("modified") }}</option></select
              ><button
                class="icon-button"
                :aria-label="t('sortDirection')"
                @click="desc = !desc"
              >
                <ArrowUpDown :size="17" />
              </button>
              <div class="view-toggle">
                <button
                  :class="{ selected: !grid }"
                  :aria-label="t('view') + ' list'"
                  @click="grid = false"
                >
                  <List :size="18" /></button
                ><button
                  :class="{ selected: grid }"
                  :aria-label="t('view') + ' grid'"
                  @click="grid = true"
                >
                  <Grid2X2 :size="17" />
                </button>
              </div>
            </div>
          </div>
          <div v-if="loading" class="empty"><span class="spinner"></span></div>
          <div v-else-if="!nodes.length" class="empty">
            <div class="empty-orbit"><img src="/Logo.png" alt="" /></div>
            <h2>
              {{
                t(
                  search
                    ? "searchEmpty"
                    : view === "recent"
                      ? "emptyList"
                      : "empty",
                )
              }}
            </h2>
            <p v-if="!search && view === 'files'">{{ t("emptyHint") }}</p>
            <button
              v-if="view === 'files' && !search"
              class="primary"
              @click="fileInput?.click()"
            >
              <UploadIcon :size="17" />{{ t("upload") }}
            </button>
          </div>
          <div v-else :class="grid ? 'file-grid' : 'file-list'">
            <div v-if="!grid" class="list-head">
              <span>{{ t("name") }}</span><span>{{ t("size") }}</span
              ><span>{{ t("modified") }}</span
              ><span></span>
            </div>
            <article
              v-for="n in nodes"
              :key="n.id"
              class="file-item"
              :class="kindClass(n)"
              @dblclick="openNode(n)"
              @contextmenu.prevent="menu = n.id"
            >
              <button class="file-main" @click="openNode(n)">
                <div class="file-symbol">
                  <img
                    v-if="grid && [Image, Film].includes(icon(n))"
                    :src="`/api/nodes/${n.id}/thumbnail`"
                    alt=""
                    @error="
                      ($event.target as HTMLImageElement).style.display = 'none'
                    "
                  /><component :is="icon(n)" :size="grid ? 42 : 24" />
                </div>
                <span class="file-name" :title="n.name">{{
                  n.name
                }}</span></button
              ><span class="file-size">{{
                n.kind === "folder" ? "—" : bytes(n.size)
              }}</span
              ><span class="file-date">{{ date(n.mtime) }}</span>
              <div class="file-actions">
                <button
                  class="icon-button"
                  :aria-label="t('details') + ' ' + n.name"
                  @click.stop="menu = menu === n.id ? '' : n.id"
                >
                  <MoreHorizontal :size="20" />
                </button>
                <div v-if="menu === n.id" class="context-menu" @click.stop>
                  <button @click="openNode(n)">
                    <component
                      :is="n.kind === 'folder' ? Folder : ExternalLink"
                      :size="16"
                    />{{ t("preview") }}</button
                  ><button v-if="n.kind === 'file'" @click="download(n)">
                    <ArrowDownToLine :size="16" />{{ t("download") }}</button
                  ><button @click="shareNode(n)">
                    <Link :size="16" />{{ t("share") }}
                  </button>
                  <hr />
                  <button @click="renameNode(n)">
                    <FileText :size="16" />{{ t("rename") }}</button
                  ><button @click="startMove(n)">
                    <Folder :size="16" />{{ t("move") }}</button
                  ><button class="danger-text" @click="deleteNode(n)">
                    <Trash2 :size="16" />{{ t("delete") }}
                  </button>
                </div>
              </div>
            </article>
          </div>
        </template>
        <section v-else-if="view === 'shares'" class="panel">
          <div v-if="!shareRows.length" class="empty">
            <Link :size="42" />
            <p>{{ t("emptyList") }}</p>
          </div>
          <article v-for="s in shareRows" :key="s.id" class="data-row">
            <div class="row-icon"><Link :size="22" /></div>
            <div class="grow">
              <strong>{{ s.name || t("home") }}</strong><small>{{
                s.revoked
                  ? t("revoked")
                  : s.expires
                    ? date(s.expires)
                    : t("permanent")
              }}</small>
            </div>
            <button
              v-if="s.url && !s.revoked"
              @click="copy(locationOrigin + s.url)"
            >
              <Copy :size="14" />{{ t("copy") }}</button
            ><button v-if="!s.revoked" @click="editExpiry(s)">
              {{ t("editExpiry") }}</button
            ><button v-if="!s.revoked" class="danger" @click="revoke(s)">
              {{ t("revoke") }}
            </button>
          </article>
        </section>
        <section v-else-if="view === 'trash'" class="panel">
          <div v-if="!trashRows.length" class="empty">
            <Trash2 :size="42" />
            <p>{{ t("trashEmpty") }}</p>
          </div>
          <article v-for="n in trashRows" :key="n.id" class="data-row">
            <component :is="icon(n)" :size="26" :class="kindClass(n)" />
            <div class="grow">
              <strong>{{ n.name }}</strong><small
                >{{ date(n.deleted_at) }} · {{ bytes(n.size) }}</small
              >
            </div>
            <button @click="restoreNode(n)">
              <RotateCcw :size="16" />{{ t("restore") }}</button
            ><button
              class="danger icon-button"
              :aria-label="t('purge')"
              @click="purgeNode(n)"
            >
              <Trash2 :size="17" />
            </button>
          </article>
        </section>
        <div v-else-if="view === 'settings'" class="settings-grid">
          <section class="panel">
            <h2><Sun :size="20" />{{ t("appearance") }}</h2>
            <div class="setting-row">
              <label>{{ t("language") }}</label><select v-model="locale">
                <option value="zh">简体中文</option>
                <option value="en">English</option>
              </select>
            </div>
            <div class="setting-row">
              <label>{{ t("appearance") }}</label><select v-model="theme">
                <option value="light">{{ t("light") }}</option>
                <option value="dark">{{ t("dark") }}</option>
                <option value="system">{{ t("system") }}</option>
              </select>
            </div>
          </section>
          <section v-if="system" class="panel">
            <h2><HardDrive :size="20" />{{ t("storage") }}</h2>
            <p class="disk-status">
              <span class="online-dot"></span>{{ t("online") }}
            </p>
            <div class="meter">
              <i
                :style="{
                  width:
                    Math.min(
                      100,
                      (system.storage.used / system.storage.total) * 100,
                    ) + '%',
                }"
              ></i>
            </div>
            <p>
              {{ bytes(system.storage.used) }} /
              {{ bytes(system.storage.total) }}
            </p>
            <small class="muted"
              >{{ t("systemInfo") }}: {{ system.system.version }} ·
              {{ system.system.os }} / {{ system.system.architecture }}</small
            >
          </section>
          <section class="panel full about-panel">
            <h2><Cloud :size="20" />{{ t("about") }}</h2>
            <div class="about-brand">
              <img src="/Logo.png" alt="" />
              <div>
                <span class="eyebrow">{{ t("productBrand") }}</span>
                <h3>{{ t("productTagline") }}</h3>
              </div>
            </div>
            <details>
              <summary>{{ t("productLead") }}</summary>
              <div class="brand-story">
                <p>{{ t("productIntro") }}</p>
                <p>{{ t("productCare") }}</p>
                <p>{{ t("productEase") }}</p>
                <p>{{ t("privacy") }}</p>
                <p>{{ t("productClosing") }}</p>
              </div>
            </details>
          </section>
          <section class="panel full">
            <div class="panel-heading">
              <h2><HardDrive :size="20" />{{ t("backups") }}</h2>
              <button @click="createBackup">
                <RefreshCw :size="16" />{{ t("createBackup") }}
              </button>
            </div>
            <p class="muted">{{ t("backupHint") }}</p>
            <div v-for="b in backupRows" :key="b" class="data-row">
              <File :size="18" /><span class="grow mono">{{ b }}</span
              ><a class="button" :href="'/api/settings/backups/' + b"
                ><ArrowDownToLine :size="16" />{{ t("download") }}</a
              >
            </div>
          </section>
          <section class="panel full">
            <h2><ShieldCheck :size="20" />{{ t("loginLogs") }}</h2>
            <div class="audit-table">
              <div class="audit-row">
                <strong>{{ t("time") }}</strong><strong>{{ t("ip") }}</strong
                ><strong>{{ t("login") }}</strong>
              </div>
              <div v-for="(l, i) in logs" :key="i" class="audit-row">
                <span>{{ date(l.time) }}</span><span>{{ l.ip }}</span
                ><span :class="l.success ? 'success-text' : 'danger-text'">{{
                  t(l.success ? "success" : "failure")
                }}</span>
              </div>
            </div>
          </section>
        </div>
      </div>
    </div>
    <section
      v-if="transfer.tasks.length"
      class="transfer-panel"
      :class="{ expanded: transfer.expanded }"
    >
      <button
        class="transfer-title"
        @click="transfer.expanded = !transfer.expanded"
      >
        <span class="transfer-icon"><UploadIcon :size="19" /></span
        ><strong>{{ t("transfers") }}</strong
        ><span class="count">{{ active }}</span
        ><ChevronDown :size="18" :class="{ rotated: transfer.expanded }" />
      </button>
      <div v-if="transfer.expanded" class="transfer-body">
        <p class="transfer-hint">{{ t("transferHint") }}</p>
        <div
          v-for="task in transfer.tasks"
          :key="task.key"
          class="transfer-task"
        >
          <div class="transfer-task-top">
            <File :size="21" /><strong :title="task.name">{{
              task.name
            }}</strong
            ><span class="task-state" :class="task.status">{{
              t(task.status)
            }}</span>
          </div>
          <div class="meter">
            <i
              :style="{
                width:
                  (task.size
                    ? (task.done / task.size) * 100
                    : task.status === 'complete'
                      ? 100
                      : 0) + '%',
              }"
            ></i>
          </div>
          <div class="task-info">
            <small
              >{{ bytes(task.done) }} / {{ bytes(task.size) }} ·
              {{
                task.size ? Math.floor((task.done / task.size) * 100) : 0
              }}%</small
            ><small v-if="task.status === 'uploading' && task.speed"
              >{{ bytes(task.speed) }}/s ·
              {{ Math.ceil((task.size - task.done) / task.speed) }}s
              {{ t("remaining") }}</small
            ><small v-if="task.status === 'hashing'"
              >{{
                Math.round((task.hashProgress / Math.max(1, task.size)) * 100)
              }}%</small
            >
          </div>
          <p v-if="task.error" class="danger-text">{{ t(task.error) }}</p>
          <div class="task-controls">
            <button
              v-if="['queued', 'hashing', 'uploading'].includes(task.status)"
              @click="pause(task)"
            >
              <Pause :size="14" />{{ t("pause") }}</button
            ><button
              v-if="task.status === 'paused'"
              :disabled="!!task.controller"
              @click="resume(task)"
            >
              <Play :size="14" />{{ t("resume") }}</button
            ><button v-if="task.status === 'failed'" @click="taskRetry(task)">
              <RotateCcw :size="14" />{{ t("retry") }}</button
            ><button
              v-if="task.status === 'recoverable'"
              @click="chooseOriginal(task)"
            >
              <UploadIcon :size="14" />{{ t("reselect") }}</button
            ><button
              v-if="!['complete', 'cancelled'].includes(task.status)"
              @click="cancelTransfer(task)"
            >
              <X :size="14" />{{ t("cancel") }}</button
            ><Check
              v-if="task.status === 'complete'"
              class="success-text"
              :size="18"
            />
          </div>
        </div>
        <button class="text-button clear-tasks" @click="clearFinished">
          {{ t("clearDone") }}
        </button>
      </div>
    </section>
  </div>
  <input
    ref="fileInput"
    type="file"
    multiple
    hidden
    @change="uploadSelected"
  /><input
    ref="folderInput"
    type="file"
    webkitdirectory
    multiple
    hidden
    @change="uploadSelected"
  /><input ref="recoverInput" type="file" hidden @change="chosenOriginal" />
  <div v-if="toast" role="status" class="toast">
    <Check :size="17" />{{ toast }}
  </div>
  <dialog ref="modal" class="dialog" @cancel.prevent="answer(null)">
    <form @submit.prevent="answer(modalInput ? modalValue : 'yes')">
      <div class="dialog-heading">
        <h2>{{ modalTitle }}</h2>
        <button
          type="button"
          class="icon-button"
          :aria-label="t('close')"
          @click="answer(null)"
        >
          <X :size="20" />
        </button>
      </div>
      <p v-if="modalHint" class="muted">{{ modalHint }}</p>
      <input
        v-if="modalInput"
        v-model="modalValue"
        autofocus
        :aria-label="modalTitle"
      />
      <div class="dialog-actions">
        <button type="button" @click="answer(null)">
          {{ t("cancel") }}</button><button
          v-for="choice in modalChoices"
          :key="choice.value"
          type="button"
          :class="choice.danger ? 'danger' : 'primary'"
          @click="answer(modalInput ? modalValue : choice.value)"
        >
          {{ choice.label }}
        </button>
      </div>
    </form>
  </dialog>
  <dialog ref="shareDialog" class="dialog">
    <div class="dialog-heading">
      <h2>{{ t("share") }}</h2>
      <button
        class="icon-button"
        :aria-label="t('close')"
        @click="shareDialog?.close()"
      >
        <X :size="20" />
      </button>
    </div>
    <p class="muted">{{ t("shareCreated") }}</p>
    <input :value="shareLink" readonly :aria-label="t('share')" />
    <div class="dialog-actions">
      <button class="primary" @click="copy(shareLink)">
        <Copy :size="16" />{{ t("copy") }}
      </button>
    </div>
  </dialog>
  <dialog ref="moveDialog" class="dialog">
    <div class="dialog-heading">
      <h2>{{ t("chooseDestination") }}</h2>
      <button
        class="icon-button"
        :aria-label="t('close')"
        @click="moveDialog?.close()"
      >
        <X :size="20" />
      </button>
    </div>
    <div class="breadcrumbs">
      <button v-for="b in moveCrumbs" :key="b.id" @click="loadMove(b.id)">
        {{ b.id === "root" ? t("home") : b.name }}<ChevronRight :size="13" />
      </button>
    </div>
    <div class="move-folders">
      <button v-for="n in moveNodes" :key="n.id" @click="loadMove(n.id)">
        <Folder :size="20" class="folder" />{{ n.name
        }}<ChevronRight :size="16" />
      </button>
    </div>
    <div class="dialog-actions">
      <button @click="moveDialog?.close()">{{ t("cancel") }}</button><button
        class="primary"
        @click="finishMove"
      >
        {{ t("useFolder") }}
      </button>
    </div>
  </dialog>
  <dialog
    ref="previewDialog"
    class="dialog preview-dialog"
    @cancel="preview = undefined"
  >
    <template v-if="preview"
      ><div class="dialog-heading">
        <div>
          <span class="eyebrow">{{ t("details") }}</span>
          <h2>{{ preview.name }}</h2>
        </div>
        <button
          class="icon-button"
          :aria-label="t('close')"
          @click="closePreview"
        >
          <X :size="22" />
        </button>
      </div>
      <div class="preview-content">
        <img v-if="mediaType === Image" :src="previewUrl" :alt="preview.name" />
        <div v-else-if="mediaType === Film" class="video-details">
          <img
            :src="`/api/nodes/${preview.id}/thumbnail`"
            alt=""
            @error="($event.target as HTMLImageElement).style.display = 'none'"
          /><Film :size="56" />
          <p>{{ t("videoHint") }}</p>
        </div>
        <audio
          v-else-if="mediaType === Music"
          :src="previewUrl"
          controls
        ></audio
        ><iframe
          v-else-if="preview.name.toLowerCase().endsWith('.pdf')"
          :src="previewUrl"
          :title="preview.name"
        ></iframe>
        <pre v-else-if="previewText">{{ previewText }}</pre>
        <p v-else class="muted">{{ previewError || t("working") }}</p>
      </div>
      <div class="preview-footer">
        <span>{{ bytes(preview.size) }} · {{ date(preview.mtime) }}</span
        ><button class="primary" @click="download(preview)">
          <ArrowDownToLine :size="17" />{{ t("download") }}
        </button>
      </div></template
    >
  </dialog>
</template>
