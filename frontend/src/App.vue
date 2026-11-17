<script setup lang="ts">
import { copyText } from "./browser";
import heroImage from "./assets/Hero.png";
import { fileKind } from "./file-kind";
import MediaThumbnail from "./MediaThumbnail.vue";
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
  FileSpreadsheet,
  Info,
  TriangleAlert,
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
const currentYear = new Date().getFullYear();
const appVersion = import.meta.env.VITE_APP_VERSION || "1.0.0";
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
const logoutBusy = ref(false),
  logoutError = ref(""),
  logoutDialog = ref<HTMLDialogElement>();
const backupBusy = ref(false);
const batchBusy = ref(false),
  movingMany = ref<string[]>([]);
const compactQuery = matchMedia("(max-width: 767px)");
const compactLayout = ref(compactQuery.matches);
const sidebarElement = ref<HTMLElement>();
const navigationButton = ref<HTMLButtonElement>();
const searchInput = ref<HTMLInputElement>();
const selectedIds = ref(new Set<string>());
const allSelected = computed(
  () =>
    nodes.value.length > 0 &&
    nodes.value.every((n) => selectedIds.value.has(n.id)),
);
const toastTone = ref<"success" | "info" | "warning" | "danger">("success");
const toastHost = ref<HTMLElement | string>("body");
let menuTrigger: HTMLElement | null = null;
let drawerReturnFocus: HTMLElement | null = null;
const dialogTriggers = new WeakMap<HTMLDialogElement, HTMLElement>();
function dialogTrigger() {
  const active = document.activeElement as HTMLElement | null;
  return active?.closest(".context-menu") ? menuTrigger : active;
}
function openDialog(dialog?: HTMLDialogElement, trigger = dialogTrigger()) {
  if (!dialog) return;
  if (trigger) dialogTriggers.set(dialog, trigger);
  dialog.showModal();
}
function restoreDialogFocus(event: Event) {
  const target = dialogTriggers.get(event.target as HTMLDialogElement);
  if (!document.querySelector("dialog[open]")) {
    if (target?.isConnected) target.focus();
    else document.getElementById("workspace-title")?.focus();
  }
}
function selectNode(id: string) {
  const ids = new Set(selectedIds.value);
  if (ids.has(id)) ids.delete(id);
  else ids.add(id);
  selectedIds.value = ids;
}
function selectAll() {
  selectedIds.value = allSelected.value
    ? new Set()
    : new Set(nodes.value.map((n) => n.id));
}
function openFileMenu(id: string, event: Event) {
  const target = event.currentTarget as HTMLElement;
  menuTrigger = target.matches("button")
    ? target
    : target.querySelector<HTMLButtonElement>(".file-actions > button");
  menu.value = menu.value === id ? "" : id;
}
function menuKeydown(event: KeyboardEvent) {
  const buttons = Array.from(
    (event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>(
      "button",
    ),
  );
  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    menu.value = "";
    menuTrigger?.focus();
  } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? buttons.length - 1
          : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) %
            buttons.length;
    buttons[next]?.focus();
  }
}
function pageKeydown(event: KeyboardEvent) {
  if (compactLayout.value && sidebar.value) {
    if (event.key === "Escape") {
      event.preventDefault();
      sidebar.value = false;
    }
    if (event.key === "Tab") {
      const items = Array.from(
        sidebarElement.value?.querySelectorAll<HTMLElement>(
          "a[href],button:not(:disabled)",
        ) || [],
      );
      const first = items[0],
        last = items.at(-1);
      if (!sidebarElement.value?.contains(document.activeElement)) {
        event.preventDefault();
        first?.focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    }
    return;
  }
  const active = document.activeElement;
  if (
    event.key === "/" &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey &&
    !document.querySelector("dialog[open]") &&
    !(
      active instanceof HTMLElement &&
      (active.matches("input,textarea,select") || active.isContentEditable)
    )
  ) {
    if (searchInput.value) {
      event.preventDefault();
      searchInput.value.focus();
    }
  }
}
function compactChanged() {
  compactLayout.value = compactQuery.matches;
  menu.value = "";
  if (!compactLayout.value) sidebar.value = false;
}
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
const view = computed(() => String(route.name || "files"));
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
const uploadProgress = computed(() => {
  const tasks = transfer.tasks.filter(
    (x) => !["complete", "cancelled"].includes(x.status),
  );
  const total = tasks.reduce((sum, x) => sum + x.size, 0);
  return total
    ? Math.min(
        100,
        Math.floor((tasks.reduce((sum, x) => sum + x.done, 0) / total) * 100),
      )
    : 0;
});
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
  const trigger = dialogTrigger();
  modalTitle.value = title;
  modalHint.value = options.hint || "";
  modalInput.value = options.input !== undefined;
  modalValue.value = options.input || "";
  modalChoices.value = options.choices || [
    { label: t("confirm"), value: "yes" },
  ];
  await nextTick();
  openDialog(modal.value, trigger);
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
function dismissToast() {
  toast.value = "";
  clearTimeout(toastTimer);
}
function notify(value: string, tone: typeof toastTone.value = "success") {
  toast.value = value;
  toastTone.value = tone;
  toastHost.value =
    document.querySelector<HTMLDialogElement>("dialog[open]") || "body";
  clearTimeout(toastTimer);
  if (tone === "success" || tone === "info")
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
    if (view.value === "not-found") return;
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
        selectedIds.value = new Set(
          [...selectedIds.value].filter((id) =>
            nodes.value.some((n) => n.id === id),
          ),
        );
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
  if (logoutBusy.value) return;
  sidebar.value = false;
  logoutBusy.value = true;
  logoutError.value = "";
  if (!logoutDialog.value?.open) openDialog(logoutDialog.value);
  stopAll();
  const timed = async (path: string, method = "GET", data?: unknown) => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 15000);
    try {
      return await api(path, method, data, controller.signal);
    } finally {
      clearTimeout(timer);
    }
  };
  try {
    try {
      await timed("/auth/logout", "POST", {});
    } catch {
      /* A lost response is resolved by checking the session below. */
    }
    let signedOut = false;
    try {
      await timed("/auth/session");
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) signedOut = true;
      else throw error;
    }
    if (!signedOut) throw new Error("logoutNotConfirmed");
    authenticated.value = false;
    setCsrf("");
    selectedIds.value = new Set();
    logoutDialog.value?.close();
    await router.replace("/files/root");
  } catch (e) {
    logoutError.value = t("logoutNotConfirmed");
  } finally {
    logoutBusy.value = false;
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
  return {
    folder: Folder,
    image: Image,
    video: Film,
    audio: Music,
    pdf: FileText,
    word: FileText,
    excel: FileSpreadsheet,
    document: FileText,
    unknown: File,
  }[fileKind(n)];
}
function kindClass(n: Node) {
  return fileKind(n);
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
    notify(t("folderCreated"));
  } catch (e) {
    fail(e);
  }
}
async function renameNode(n: Node) {
  menu.value = "";
  const name = await ask(t("rename"), { input: n.name, hint: t("newName") });
  if (!name || name === n.name) return;
  try {
    const changed = await conflict((strategy) =>
      api(`/nodes/${n.id}`, "PATCH", { name, parent: n.parent, strategy }),
    );
    await load();
    if (changed) notify(t("fileRenamed"));
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
    notify(t("fileTrashed"));
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
      (x: Node) =>
        x.kind === "folder" &&
        x.id !== moving.value?.id &&
        !movingMany.value.includes(x.id),
    );
    moveCrumbs.value = data.breadcrumbs;
  } catch (e) {
    fail(e);
  }
}
async function startMove(n: Node) {
  const trigger = dialogTrigger();
  menu.value = "";
  moving.value = n;
  movingMany.value = [];
  await loadMove("root");
  openDialog(moveDialog.value, trigger);
}
async function finishMove() {
  if (batchBusy.value) return;
  if (movingMany.value.length) {
    const ids = [...movingMany.value];
    batchBusy.value = true;
    moveDialog.value?.close();
    try {
      const result = await api("/nodes/batch-move", "POST", {
        ids,
        parent: moveParent.value,
      });
      selectedIds.value = new Set();
      await load();
      notify(
        t(result.count ? "fileMoved" : "alreadyInFolder"),
        result.count ? "success" : "info",
      );
    } catch (error) {
      fail(error);
    } finally {
      batchBusy.value = false;
      movingMany.value = [];
    }
    return;
  }
  if (!moving.value) return;
  try {
    const n = moving.value;
    moveDialog.value?.close();
    const changed = await conflict((strategy) =>
      api(`/nodes/${n.id}`, "PATCH", {
        parent: moveParent.value,
        name: n.name,
        strategy,
      }),
    );
    await load();
    if (changed) notify(t("fileMoved"));
  } catch (e) {
    fail(e);
  }
}
async function startBatchMove() {
  if (batchBusy.value) return;
  if (selectedIds.value.size > 500) {
    fail(new Error("invalid_batch"));
    return;
  }
  const trigger = dialogTrigger();
  moving.value = undefined;
  movingMany.value = [...selectedIds.value];
  await loadMove("root");
  openDialog(moveDialog.value, trigger);
}
async function trashSelected() {
  if (batchBusy.value) return;
  if (selectedIds.value.size > 500) {
    fail(new Error("invalid_batch"));
    return;
  }
  const ids = [...selectedIds.value];
  if (
    !(await ask(t("trashSelected"), {
      hint: t("batchTrashHint").replace("{count}", String(ids.length)),
      choices: [{ label: t("trashSelected"), value: "yes", danger: true }],
    }))
  )
    return;
  batchBusy.value = true;
  try {
    await api("/nodes/batch-trash", "POST", { ids });
    selectedIds.value = new Set();
    await load();
    await refreshStorage();
    notify(t("fileTrashed"));
  } catch (error) {
    fail(error);
  } finally {
    batchBusy.value = false;
  }
}
async function restoreNode(n: any) {
  try {
    const changed = await conflict((strategy) =>
      api(`/trash/${n.id}/restore`, "POST", { strategy }),
    );
    await load();
    if (changed) notify(t("fileRestored"));
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
    notify(t("filePurged"));
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
    notify(t("trashCleared"));
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
  const trigger = dialogTrigger();
  menu.value = "";
  try {
    const expires = await days();
    if (expires === undefined) return;
    const data = await api("/shares", "POST", { root: n.id, expires });
    shareLink.value = location.origin + data.url;
    openDialog(shareDialog.value, trigger);
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
async function deleteShareRecord(s: any) {
  if (
    !(await ask(t("deleteShareRecord"), {
      hint: t("deleteShareRecordHint"),
      choices: [{ label: t("deleteShareRecord"), value: "yes", danger: true }],
    }))
  )
    return;
  try {
    await api(`/shares/${s.id}/record`, "DELETE");
    await load();
    notify(t("shareRecordDeleted"));
  } catch (e) {
    fail(e);
  }
}
async function createBackup() {
  if (backupBusy.value) return;
  backupBusy.value = true;
  try {
    await api("/settings/backups", "POST", {});
    backupRows.value = await api("/settings/backups");
  } catch (e) {
    fail(e);
  } finally {
    backupBusy.value = false;
  }
}
async function deleteBackup(name: string) {
  if (
    backupBusy.value ||
    !(await ask(t("deleteBackup"), {
      hint: `${name}\n${t("deleteBackupHint")}`,
      choices: [{ label: t("deleteBackup"), value: "yes", danger: true }],
    }))
  )
    return;
  backupBusy.value = true;
  try {
    await api(`/settings/backups/${encodeURIComponent(name)}`, "DELETE");
    backupRows.value = await api("/settings/backups");
    notify(t("backupDeleted"));
  } catch (error) {
    fail(error);
  } finally {
    backupBusy.value = false;
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
  const trigger = dialogTrigger();
  menu.value = "";
  if (n.kind === "folder") {
    search.value = "";
    router.push("/files/" + n.id);
    return;
  }
  preview.value = n;
  previewText.value = "";
  previewError.value = "";
  openDialog(previewDialog.value, trigger);
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
  menu.value = "";
  notify(t("downloadStarted"), "info");
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
watch(menu, async (id) => {
  if (!id) return;
  await nextTick();
  const panel = document.getElementById(`file-menu-${id}`);
  if (panel && panel.getBoundingClientRect().bottom > innerHeight - 16) {
    panel.style.top = "auto";
    panel.style.bottom = "100%";
  }
  panel?.querySelector<HTMLButtonElement>("button")?.focus();
});
watch(sidebar, async (open) => {
  document.body.classList.toggle("drawer-open", open && compactLayout.value);
  if (open && compactLayout.value) {
    drawerReturnFocus = document.activeElement as HTMLElement;
    await nextTick();
    sidebarElement.value?.querySelector<HTMLElement>("a[href],button")?.focus();
  } else if (compactLayout.value && drawerReturnFocus?.isConnected) {
    await nextTick();
    drawerReturnFocus.focus();
    drawerReturnFocus = null;
  }
});
watch(authenticated, (value) => {
  if (!value) sidebar.value = false;
});
watch([locale, theme], applyPreferences);
watch(
  [view, locale],
  () => {
    document.title =
      view.value === "not-found"
        ? `404 · ${t("pageNotFoundTitle")} · Magic Cloud`
        : "Magic Cloud · 麦吉云";
  },
  { immediate: true },
);
watch(
  () => route.fullPath,
  () => {
    selectedIds.value = new Set();
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
  compactQuery.addEventListener("change", compactChanged);
  document.addEventListener("keydown", pageKeydown);
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
  compactQuery.removeEventListener("change", compactChanged);
  document.removeEventListener("keydown", pageKeydown);
  document.body.classList.remove("drawer-open");
  clearTimeout(searchTimer);
  clearTimeout(toastTimer);
});
</script>

<template>
  <main v-if="view === 'not-found'" class="not-found-page">
    <header class="not-found-header">
      <RouterLink to="/" class="not-found-brand">
        <img src="/Logo.png" alt="" width="48" height="48" />
        <strong>{{ t("brand") }}</strong>
      </RouterLink>
      <div>
        <button
          class="icon-button"
          :aria-label="t('language')"
          @click="locale = locale === 'zh' ? 'en' : 'zh'"
          :data-tooltip="t('language')"
        >
          <Languages aria-hidden="true" :size="20" />
        </button>
        <button
          class="icon-button"
          :aria-label="t('appearance')"
          @click="theme = theme === 'dark' ? 'light' : 'dark'"
          :data-tooltip="t('appearance')"
        >
          <Sun aria-hidden="true" v-if="theme === 'dark'" :size="20" /><Moon
            aria-hidden="true"
            v-else
            :size="20"
          />
        </button>
      </div>
    </header>
    <section class="not-found-card" aria-labelledby="not-found-title">
      <img
        class="not-found-mascot"
        src="/Logo.png"
        :alt="t('mascotAlt')"
        width="144"
        height="144"
      />
      <span class="not-found-code">404</span>
      <h1 id="not-found-title">{{ t("pageNotFoundTitle") }}</h1>
      <p>{{ t("pageNotFoundHint") }}</p>
      <RouterLink to="/" class="button primary"
        ><ArrowLeft aria-hidden="true" :size="20" />{{
          t("returnHome")
        }}</RouterLink
      >
    </section>
    <footer class="not-found-footer">
      © {{ currentYear }} Zexiang Zhang. All rights reserved.<br />Magic Cloud
      v{{ appVersion }}
    </footer>
  </main>
  <div v-else-if="checking" class="boot">
    <img src="/Logo.png" alt="Magic Cloud" /><span class="spinner"></span>
  </div>
  <main v-else-if="view === 'public'" class="public-page">
    <header class="public-header">
      <a href="/"
        ><img src="/Logo.png" alt="" /><b>{{ t("brand") }}</b></a
      ><span
        ><ShieldCheck aria-hidden="true" :size="16" />{{ t("readOnly") }}</span
      ><button
        :aria-label="t('language')"
        :title="t('language')"
        @click="locale = locale === 'zh' ? 'en' : 'zh'"
      >
        <Languages aria-hidden="true" :size="20" />
      </button>
    </header>
    <section
      v-if="publicError"
      class="public-card share-unavailable"
      role="alert"
    >
      <div class="share-error-icon">
        <component
          aria-hidden="true"
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
          <ArrowLeft aria-hidden="true" :size="16" />{{ t("shareHome") }}
        </button>
        <button @click="load">
          <RefreshCw aria-hidden="true" :size="16" />{{ t("checkAgain") }}
        </button>
        <RouterLink class="button primary" to="/">{{ t("goHome") }}</RouterLink>
      </div>
    </section>
    <section v-if="publicData" class="public-card">
      <span class="eyebrow">{{ t("readOnly") }}</span>
      <h1>{{ publicData.root.name || t("brand") }}</h1>
      <div class="toolbar">
        <button v-if="route.query.node" @click="router.push(route.path)">
          <ArrowLeft aria-hidden="true" :size="16" />{{ t("back") }}</button
        ><span>{{ publicData.node.name }}</span>
      </div>
      <template v-if="publicData.node.kind === 'folder'"
        ><button
          v-for="n in publicData.nodes"
          :key="n.id"
          class="public-file"
          @click="publicOpen(n)"
        >
          <component
            aria-hidden="true"
            :is="icon(n)"
            :class="kindClass(n)"
            :size="24"
          /><strong>{{ n.name }}</strong
          ><small>{{ n.kind === "folder" ? t("folder") : bytes(n.size) }}</small
          ><component
            aria-hidden="true"
            :is="n.kind === 'folder' ? ChevronRight : ArrowDownToLine"
            :size="20"
          />
        </button>
        <p v-if="!publicData.nodes.length" class="muted">
          {{ t("emptyList") }}
        </p></template
      >
      <div v-else class="public-single">
        <component
          aria-hidden="true"
          :is="icon(publicData.node)"
          :size="70"
          :class="kindClass(publicData.node)"
        />
        <h2>{{ publicData.node.name }}</h2>
        <p class="muted">
          {{ bytes(publicData.node.size) }} · {{ date(publicData.node.mtime) }}
        </p>
        <button class="primary" @click="publicOpen(publicData.node)">
          <ArrowDownToLine aria-hidden="true" :size="20" />{{ t("download") }}
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
        <Languages aria-hidden="true" :size="20" /></button
      ><button
        :aria-label="t('appearance')"
        @click="theme = theme === 'dark' ? 'light' : 'dark'"
      >
        <component
          aria-hidden="true"
          :is="theme === 'dark' ? Sun : Moon"
          :size="20"
        />
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
        <ShieldCheck aria-hidden="true" :size="16" />{{ t("secure") }}
      </div>
    </section>
    <section class="login-form-wrap">
      <form class="login-form" @submit.prevent="signIn">
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
          :aria-invalid="!!error"
          :aria-describedby="error ? 'login-error' : undefined"
        />
        <div v-if="error" id="login-error" role="alert" class="notice error">
          <AlertCircle aria-hidden="true" :size="16" />{{ error }}
        </div>
        <button class="primary" :disabled="loginBusy || !password">
          <span v-if="loginBusy" class="spinner"></span
          ><span v-else>{{ t("login") }}</span
          ><ChevronRight aria-hidden="true" :size="20" />
        </button>
        <p class="login-privacy">
          <ShieldCheck aria-hidden="true" :size="16" />{{ t("privacy") }}
        </p>
      </form>
      <footer class="login-footer">
        <span>© {{ currentYear }} Zexiang Zhang. All rights reserved.</span>
        <span>Magic Cloud v{{ appVersion }}</span>
      </footer>
    </section>
  </main>
  <div v-else class="app-shell" @click="menu = ''">
    <div
      v-if="sidebar && compactLayout"
      class="sidebar-backdrop"
      @click="sidebar = false"
      aria-hidden="true"
    ></div>
    <aside
      ref="sidebarElement"
      id="navigation-drawer"
      class="sidebar"
      :class="{ open: sidebar }"
      :inert="compactLayout && !sidebar"
      :role="compactLayout && sidebar ? 'dialog' : undefined"
      :aria-modal="compactLayout && sidebar ? true : undefined"
      :aria-label="t('navigation')"
    >
      <button
        v-if="compactLayout"
        class="icon-button drawer-close"
        :aria-label="t('closeNavigation')"
        @click="sidebar = false"
        :data-tooltip="t('closeNavigation')"
      >
        <X aria-hidden="true" :size="20" />
      </button>
      <a class="brand" href="/" @click.prevent="router.push('/files/root')"
        ><img src="/Logo.png" alt="" /><span>{{ t("brand") }}</span></a
      ><span class="nav-label">{{
        locale === "zh" ? "我的空间" : "WORKSPACE"
      }}</span>
      <nav>
        <RouterLink
          v-for="item in nav"
          :key="item.key"
          :to="item.path"
          :class="{ active: view === item.key }"
          ><component aria-hidden="true" :is="item.icon" :size="20" /><span>{{
            t(item.key)
          }}</span></RouterLink
        >
      </nav>
      <div class="sidebar-bottom">
        <section v-if="system" class="storage-mini">
          <div>
            <HardDrive aria-hidden="true" :size="20" /><span>{{
              t("storage")
            }}</span
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
              aria-hidden="true"
              :is="theme === 'dark' ? Moon : theme === 'light' ? Sun : Monitor"
              :size="20"
            /></button
          ><button
            :aria-label="t('language')"
            :title="t('language')"
            @click="locale = locale === 'zh' ? 'en' : 'zh'"
          >
            <Languages aria-hidden="true" :size="20" /></button
          ><button :aria-label="t('logout')" @click="signOut">
            <LogOut aria-hidden="true" :size="20" />
          </button>
        </div>
      </div>
    </aside>
    <div class="workspace" :inert="compactLayout && sidebar">
      <header class="topbar">
        <button
          class="mobile-only icon-button"
          ref="navigationButton"
          :aria-label="t('menu')"
          aria-controls="navigation-drawer"
          :aria-expanded="sidebar"
          @click="sidebar = !sidebar"
          :data-tooltip="t('menu')"
        >
          <Menu aria-hidden="true" :size="24" />
        </button>
        <div class="search" v-if="['files', 'recent'].includes(view)">
          <Search aria-hidden="true" :size="20" /><input
            ref="searchInput"
            v-model="search"
            :placeholder="t('search')"
            :aria-label="t('search')"
          /><button
            v-if="search"
            class="icon-button"
            :aria-label="t('clearSearch')"
            @click="
              search = '';
              searchInput?.focus();
            "
            :data-tooltip="t('clearSearch')"
          >
            <X aria-hidden="true" :size="16" /></button
          ><kbd aria-hidden="true">/</kbd>
        </div>
        <span v-else class="topbar-label">{{ t("brand") }}</span>
        <div class="topbar-status">
          <span class="online-dot"></span><span>{{ t("secure") }}</span>
        </div>
      </header>
      <div
        class="content"
        @dragover.prevent="view === 'files' && (drop = true)"
        @dragleave.self="drop = false"
        @drop.prevent="dropped"
      >
        <div
          v-if="drop && view === 'files'"
          class="drop-overlay"
          aria-hidden="true"
        >
          <UploadIcon aria-hidden="true" :size="48" />
          <h2>{{ t("drop") }}</h2>
        </div>
        <div class="page-heading">
          <div>
            <div v-if="view === 'files'" class="breadcrumbs">
              <button
                v-for="(b, i) in breadcrumbs"
                :key="b.id"
                @click="router.push('/files/' + b.id)"
                :aria-current="
                  i === breadcrumbs.length - 1 ? 'page' : undefined
                "
              >
                <ChevronRight aria-hidden="true" v-if="i" :size="16" />{{
                  b.id === "root" ? t("home") : b.name
                }}
              </button>
            </div>
            <h1 id="workspace-title" tabindex="-1">{{ title }}</h1>
            <p v-if="view === 'files'" class="muted">
              {{ nodes.length }} {{ locale === "zh" ? "个项目" : "items" }}
              <span class="dot-separator">·</span> {{ t("welcome") }}
            </p>
            <p v-if="view === 'trash'" class="muted">{{ t("trashHint") }}</p>
            <p v-if="view === 'shares'" class="muted">
              {{ t("emptySharesHint") }}
            </p>
          </div>
          <div class="heading-actions">
            <button
              v-if="view === 'files'"
              :aria-label="t('newFolder')"
              class="new-folder-button"
              @click="makeFolder"
            >
              <FolderPlus aria-hidden="true" :size="20" /><span>{{
                t("newFolder")
              }}</span></button
            ><button
              v-if="view === 'files'"
              class="primary"
              @click="fileInput?.click()"
            >
              <UploadIcon aria-hidden="true" :size="20" />{{
                t("upload")
              }}</button
            ><button
              v-if="view === 'files'"
              class="folder-upload-button"
              @click="folderInput?.click()"
            >
              <FolderUp aria-hidden="true" :size="20" />{{
                t("uploadFolder")
              }}</button
            ><button
              v-if="view === 'trash' && trashRows.length"
              class="danger"
              @click="emptyTrash"
            >
              <Trash2 aria-hidden="true" :size="20" />{{ t("emptyTrash") }}
            </button>
          </div>
        </div>
        <div v-if="error" role="alert" class="notice error">
          <AlertCircle aria-hidden="true" :size="20" /><span>{{ error }}</span
          ><button :aria-label="t('close')" @click="error = ''">
            <X aria-hidden="true" :size="16" />
          </button>
        </div>
        <template v-if="['files', 'recent'].includes(view)"
          ><div class="toolbar">
            <div class="selection-status" v-if="selectedIds.size">
              <span role="status"
                >{{ selectedIds.size }} {{ t("selectedItems") }}</span
              >
              <button class="text-button" @click="selectedIds = new Set()">
                {{ t("clearSelection") }}
              </button>
              <button :disabled="batchBusy" @click="startBatchMove">
                <Folder aria-hidden="true" :size="16" />{{ t("moveSelected") }}
              </button>
              <button
                class="danger"
                :disabled="batchBusy"
                @click="trashSelected"
              >
                <Trash2 aria-hidden="true" :size="16" />{{ t("trashSelected") }}
              </button>
            </div>
            <div v-else class="filter-label">
              <Folder aria-hidden="true" :size="16" />{{
                search ? t("search").replace("…", "") : t("home")
              }}
            </div>
            <div class="toolbar-right">
              <div class="sort-control">
                <label for="file-sort">{{ t("sortBy") }}</label><select
                  id="file-sort"
                  v-model="sort"
                >
                  <option value="name">{{ t("name") }}</option>
                  <option value="size">{{ t("size") }}</option>
                  <option value="type">{{ t("type") }}</option>
                  <option value="mtime">{{ t("modified") }}</option>
                </select>
              </div>
              <button
                class="icon-button"
                :aria-label="t('sortDirection')"
                @click="desc = !desc"
                :data-tooltip="t('sortDirection')"
              >
                <ArrowUpDown aria-hidden="true" :size="20" />
              </button>
              <div class="view-toggle">
                <button
                  :class="{ selected: !grid }"
                  :aria-pressed="!grid"
                  :aria-label="t('view') + ' list'"
                  @click="grid = false"
                >
                  <List aria-hidden="true" :size="20" /></button
                ><button
                  :class="{ selected: grid }"
                  :aria-pressed="grid"
                  :aria-label="t('view') + ' grid'"
                  @click="grid = true"
                >
                  <Grid2X2 aria-hidden="true" :size="20" />
                </button>
              </div>
            </div>
          </div>

          <div v-if="loading" class="empty"><span class="spinner"></span></div>
          <div
            v-else-if="!nodes.length && (view !== 'files' || !!search)"
            class="empty"
          >
            <div class="empty-orbit">
              <img src="/Logo.png" :alt="t('mascotAlt')" />
            </div>
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
              <UploadIcon aria-hidden="true" :size="20" />{{ t("upload") }}
            </button>
          </div>
          <div v-else :class="grid ? 'file-grid' : 'file-list'">
            <div v-if="!grid" class="list-head">
              <input
                type="checkbox"
                class="file-selection"
                :checked="allSelected"
                :indeterminate="selectedIds.size > 0 && !allSelected"
                :aria-label="t('selectAll')"
                @change="selectAll"
              />
              <span>{{ t("name") }}</span><span>{{ t("size") }}</span
              ><span>{{ t("modified") }}</span
              ><span></span>
            </div>
            <button
              v-if="view === 'files' && !search"
              class="upload-dropzone"
              :class="{ dragging: drop }"
              @click="fileInput?.click()"
            >
              <UploadIcon aria-hidden="true" :size="24" />
              <span>{{ active ? t("uploading") : t("dropzoneHint") }}</span>
              <span
                v-if="active"
                class="dropzone-progress meter"
                role="progressbar"
                :aria-label="t('transfers')"
                :aria-valuenow="uploadProgress"
                aria-valuemin="0"
                aria-valuemax="100"
                ><i :style="{ width: uploadProgress + '%' }"></i
              ></span>
              <small v-if="active">{{ uploadProgress }}%</small>
            </button>
            <article
              v-for="n in nodes"
              :key="n.id"
              class="file-item"
              :class="[kindClass(n), { 'is-selected': selectedIds.has(n.id) }]"
              @dblclick="openNode(n)"
              @contextmenu.prevent.stop="openFileMenu(n.id, $event)"
            >
              <input
                type="checkbox"
                class="file-selection"
                :checked="selectedIds.has(n.id)"
                :aria-label="t('selectItem') + ' ' + n.name"
                @change="selectNode(n.id)"
                @click.stop
                @dblclick.stop
              />
              <button class="file-main" @click="openNode(n)">
                <div class="file-symbol">
                  <MediaThumbnail
                    v-if="grid && [Image, Film].includes(icon(n))"
                    :id="n.id"
                    :name="n.name"
                    :video="icon(n) === Film"
                  /><component
                    aria-hidden="true"
                    :is="icon(n)"
                    :size="grid ? 48 : 24"
                  />
                </div>
                <span class="file-name" :title="n.name">{{
                  n.name
                }}</span></button
              ><span class="file-size">{{
                n.kind === "folder" ? "—" : bytes(n.size)
              }}</span
              ><span class="file-date"
                ><span class="sr-only">{{ t("modified") }}: </span
                >{{ date(n.mtime) }}</span
              >
              <div class="file-actions">
                <button
                  class="icon-button"
                  :aria-label="t('fileActions') + ' ' + n.name"
                  aria-haspopup="menu"
                  :aria-expanded="menu === n.id"
                  :aria-controls="`file-menu-${n.id}`"
                  @click.stop="openFileMenu(n.id, $event)"
                  @keydown.down.prevent.stop="openFileMenu(n.id, $event)"
                  :data-tooltip="t('fileActions') + ' ' + n.name"
                >
                  <MoreHorizontal aria-hidden="true" :size="20" />
                </button>
                <div
                  v-if="menu === n.id"
                  :id="`file-menu-${n.id}`"
                  class="context-menu"
                  role="menu"
                  :aria-label="t('fileActions') + ' ' + n.name"
                  @click.stop
                  @keydown="menuKeydown"
                >
                  <button role="menuitem" @click="openNode(n)">
                    <component
                      aria-hidden="true"
                      :is="n.kind === 'folder' ? Folder : ExternalLink"
                      :size="16"
                    />{{ t("preview") }}</button
                  ><button
                    role="menuitem"
                    v-if="n.kind === 'file'"
                    @click="download(n)"
                  >
                    <ArrowDownToLine aria-hidden="true" :size="16" />{{
                      t("download")
                    }}</button
                  ><button role="menuitem" @click="shareNode(n)">
                    <Link aria-hidden="true" :size="16" />{{ t("share") }}
                  </button>
                  <hr />
                  <button role="menuitem" @click="renameNode(n)">
                    <FileText aria-hidden="true" :size="16" />{{
                      t("rename")
                    }}</button
                  ><button role="menuitem" @click="startMove(n)">
                    <Folder aria-hidden="true" :size="16" />{{
                      t("move")
                    }}</button
                  ><button
                    role="menuitem"
                    class="danger-text"
                    @click="deleteNode(n)"
                  >
                    <Trash2 aria-hidden="true" :size="16" />{{ t("delete") }}
                  </button>
                </div>
              </div>
            </article>
          </div>
        </template>
        <section v-else-if="view === 'shares'" class="panel">
          <div v-if="!shareRows.length" class="empty">
            <div class="empty-orbit">
              <img src="/Logo.png" :alt="t('mascotAlt')" />
            </div>
            <h2>{{ t("emptyShares") }}</h2>
            <RouterLink to="/files/root" class="button primary">{{
              t("browseFiles")
            }}</RouterLink>
          </div>
          <article v-for="s in shareRows" :key="s.id" class="data-row">
            <div class="row-icon"><Link aria-hidden="true" :size="24" /></div>
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
              <Copy aria-hidden="true" :size="16" />{{ t("copy") }}</button
            ><button v-if="!s.revoked" @click="editExpiry(s)">
              {{ t("editExpiry") }}</button
            ><button v-if="!s.revoked" class="danger" @click="revoke(s)">
              {{ t("revoke") }}</button
            ><button v-else class="danger" @click="deleteShareRecord(s)">
              <Trash2 aria-hidden="true" :size="16" />{{
                t("deleteShareRecord")
              }}
            </button>
          </article>
        </section>
        <section v-else-if="view === 'trash'" class="panel">
          <div v-if="!trashRows.length" class="empty">
            <div class="empty-orbit">
              <img src="/Logo.png" :alt="t('mascotAlt')" />
            </div>
            <h2>{{ t("trashEmpty") }}</h2>
            <RouterLink to="/files/root" class="button primary">{{
              t("browseFiles")
            }}</RouterLink>
          </div>
          <article v-for="n in trashRows" :key="n.id" class="data-row">
            <component
              aria-hidden="true"
              :is="icon(n)"
              :size="24"
              :class="kindClass(n)"
            />
            <div class="grow">
              <strong>{{ n.name }}</strong><small
                >{{ date(n.deleted_at) }} · {{ bytes(n.size) }}</small
              >
            </div>
            <button @click="restoreNode(n)">
              <RotateCcw aria-hidden="true" :size="16" />{{
                t("restore")
              }}</button
            ><button
              class="danger icon-button"
              :aria-label="t('purge')"
              @click="purgeNode(n)"
              :data-tooltip="t('purge')"
            >
              <Trash2 aria-hidden="true" :size="20" />
            </button>
          </article>
        </section>
        <div v-else-if="view === 'settings'" class="settings-grid">
          <section class="panel">
            <h2><Sun aria-hidden="true" :size="20" />{{ t("appearance") }}</h2>
            <div class="setting-row">
              <label for="language-preference">{{
                t("language")
              }}</label><select id="language-preference" v-model="locale">
                <option value="zh">简体中文</option>
                <option value="en">English</option>
              </select>
            </div>
            <div class="setting-row">
              <label for="theme-preference">{{ t("appearance") }}</label><select
                id="theme-preference"
                v-model="theme"
              >
                <option value="light">{{ t("light") }}</option>
                <option value="dark">{{ t("dark") }}</option>
                <option value="system">{{ t("system") }}</option>
              </select>
            </div>
          </section>
          <section v-if="system" class="panel">
            <h2>
              <HardDrive aria-hidden="true" :size="20" />{{ t("storage") }}
            </h2>
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
            <h2><Cloud aria-hidden="true" :size="20" />{{ t("about") }}</h2>
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
              <h2>
                <HardDrive aria-hidden="true" :size="20" />{{ t("backups") }}
              </h2>
              <button :disabled="backupBusy" @click="createBackup">
                <RefreshCw aria-hidden="true" :size="16" />{{
                  t("createBackup")
                }}
              </button>
            </div>
            <p class="muted">{{ t("backupHint") }}</p>
            <div v-for="b in backupRows" :key="b" class="data-row">
              <File aria-hidden="true" :size="20" /><span
                class="grow mono backup-name"
                >{{ b }}</span
              ><a class="button" :href="'/api/settings/backups/' + b"
                ><ArrowDownToLine aria-hidden="true" :size="16" />{{
                  t("download")
                }}</a
              >
              <button
                class="danger"
                :disabled="backupBusy"
                :aria-label="t('deleteBackup') + ' ' + b"
                @click="deleteBackup(b)"
              >
                <Trash2 :size="16" aria-hidden="true" />{{ t("deleteBackup") }}
              </button>
            </div>
          </section>
          <section class="panel full">
            <h2>
              <ShieldCheck aria-hidden="true" :size="20" />{{ t("loginLogs") }}
            </h2>
            <p class="muted">{{ t("loginLogsHint") }}</p>
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
      :inert="compactLayout && sidebar"
      :class="{ expanded: transfer.expanded }"
    >
      <button
        class="transfer-title"
        :aria-expanded="transfer.expanded"
        aria-controls="transfer-tasks"
        @click="transfer.expanded = !transfer.expanded"
      >
        <span class="transfer-icon"
          ><UploadIcon aria-hidden="true" :size="20" /></span
        ><strong>{{ t("transfers") }}</strong
        ><span class="count">{{ active }}</span
        ><ChevronDown
          aria-hidden="true"
          :size="20"
          :class="{ rotated: transfer.expanded }"
        />
      </button>
      <div v-if="transfer.expanded" id="transfer-tasks" class="transfer-body">
        <p class="transfer-hint">{{ t("transferHint") }}</p>
        <div
          v-for="task in transfer.tasks"
          :key="task.key"
          class="transfer-task"
        >
          <div class="transfer-task-top">
            <File aria-hidden="true" :size="20" /><strong :title="task.name">{{
              task.name
            }}</strong
            ><span class="task-state" :class="task.status" role="status">{{
              t(task.status)
            }}</span>
          </div>
          <div
            class="meter"
            :class="task.status"
            role="progressbar"
            :aria-label="task.name"
            aria-valuemin="0"
            aria-valuemax="100"
            :aria-valuenow="
              task.size
                ? Math.min(100, Math.floor((task.done / task.size) * 100))
                : task.status === 'complete'
                  ? 100
                  : 0
            "
          >
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
              <Pause aria-hidden="true" :size="16" />{{ t("pause") }}</button
            ><button
              v-if="task.status === 'paused'"
              :disabled="!!task.controller"
              @click="resume(task)"
            >
              <Play aria-hidden="true" :size="16" />{{ t("resume") }}</button
            ><button v-if="task.status === 'failed'" @click="taskRetry(task)">
              <RotateCcw aria-hidden="true" :size="16" />{{
                t("retry")
              }}</button
            ><button
              v-if="task.status === 'recoverable'"
              @click="chooseOriginal(task)"
            >
              <UploadIcon aria-hidden="true" :size="16" />{{
                t("reselect")
              }}</button
            ><button
              v-if="!['complete', 'cancelled'].includes(task.status)"
              @click="cancelTransfer(task)"
            >
              <X aria-hidden="true" :size="16" />{{ t("cancel") }}</button
            ><Check
              aria-hidden="true"
              v-if="task.status === 'complete'"
              class="success-text"
              :size="20"
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
  <Teleport :to="toastHost">
    <div
      v-if="toast"
      :role="
        toastTone === 'danger' || toastTone === 'warning' ? 'alert' : 'status'
      "
      class="toast"
      :class="toastTone"
    >
      <component
        aria-hidden="true"
        :is="
          {
            success: Check,
            info: Info,
            warning: TriangleAlert,
            danger: AlertCircle,
          }[toastTone]
        "
        :size="20"
      />
      <span>{{ toast }}</span><button
        class="icon-button"
        :aria-label="t('close')"
        @click="dismissToast"
        :data-tooltip="t('close')"
      >
        <X aria-hidden="true" :size="16" />
      </button>
    </div>
  </Teleport>
  <dialog
    ref="logoutDialog"
    class="dialog"
    aria-labelledby="logout-title"
    aria-describedby="logout-description"
    @cancel.prevent="!logoutBusy && logoutDialog?.close()"
    @close="restoreDialogFocus"
  >
    <h2 id="logout-title">{{ t(logoutBusy ? "loggingOut" : "logout") }}</h2>
    <p id="logout-description" role="status">
      <span v-if="logoutBusy" class="spinner"></span
      >{{ logoutError || t("logoutHint") }}
    </p>
    <div v-if="logoutError" class="dialog-actions">
      <button @click="logoutDialog?.close()">{{ t("back") }}</button><button
        class="primary"
        @click="signOut"
      >
        {{ t("retry") }}
      </button>
    </div>
  </dialog>
  <dialog
    ref="modal"
    class="dialog"
    aria-labelledby="modal-title"
    :aria-describedby="modalHint ? 'modal-description' : undefined"
    @close="restoreDialogFocus"
    @cancel.prevent="answer(null)"
  >
    <form @submit.prevent="answer(modalInput ? modalValue : 'yes')">
      <div class="dialog-heading">
        <h2 id="modal-title">{{ modalTitle }}</h2>
        <button
          type="button"
          class="icon-button"
          :aria-label="t('close')"
          @click="answer(null)"
          :data-tooltip="t('close')"
        >
          <X aria-hidden="true" :size="20" />
        </button>
      </div>
      <p v-if="modalHint" id="modal-description" class="muted">
        {{ modalHint }}
      </p>
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
  <dialog
    ref="shareDialog"
    class="dialog"
    aria-labelledby="share-title"
    aria-describedby="share-description"
    @close="restoreDialogFocus"
  >
    <div class="dialog-heading">
      <h2 id="share-title">{{ t("share") }}</h2>
      <button
        class="icon-button"
        :aria-label="t('close')"
        @click="shareDialog?.close()"
        :data-tooltip="t('close')"
      >
        <X aria-hidden="true" :size="20" />
      </button>
    </div>
    <p id="share-description" class="muted">{{ t("shareCreated") }}</p>
    <input :value="shareLink" readonly :aria-label="t('share')" />
    <div class="dialog-actions">
      <button class="primary" @click="copy(shareLink)">
        <Copy aria-hidden="true" :size="16" />{{ t("copy") }}
      </button>
    </div>
  </dialog>
  <dialog
    ref="moveDialog"
    class="dialog"
    aria-labelledby="move-title"
    aria-describedby="move-description"
    @close="restoreDialogFocus"
  >
    <div class="dialog-heading">
      <h2 id="move-title">{{ t("chooseDestination") }}</h2>
      <button
        class="icon-button"
        :aria-label="t('close')"
        @click="moveDialog?.close()"
        :data-tooltip="t('close')"
      >
        <X aria-hidden="true" :size="20" />
      </button>
    </div>
    <p id="move-description" class="muted">
      {{ t(movingMany.length ? "batchMoveHint" : "moveHint") }}
    </p>
    <div class="breadcrumbs">
      <button v-for="b in moveCrumbs" :key="b.id" @click="loadMove(b.id)">
        {{ b.id === "root" ? t("home") : b.name
        }}<ChevronRight aria-hidden="true" :size="16" />
      </button>
    </div>
    <div class="move-folders">
      <button v-for="n in moveNodes" :key="n.id" @click="loadMove(n.id)">
        <Folder aria-hidden="true" :size="20" class="folder" />{{ n.name
        }}<ChevronRight aria-hidden="true" :size="16" />
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
    aria-labelledby="preview-title"
    aria-describedby="preview-description"
    @close="restoreDialogFocus"
    @cancel="preview = undefined"
  >
    <template v-if="preview"
      ><div class="dialog-heading">
        <div>
          <span class="eyebrow">{{ t("details") }}</span>
          <h2 id="preview-title">{{ preview.name }}</h2>
        </div>
        <button
          class="icon-button"
          :aria-label="t('close')"
          @click="closePreview"
          :data-tooltip="t('close')"
        >
          <X aria-hidden="true" :size="24" />
        </button>
      </div>
      <div class="preview-content">
        <img v-if="mediaType === Image" :src="previewUrl" :alt="preview.name" />
        <MediaThumbnail
          v-else-if="mediaType === Film"
          :id="preview.id"
          :name="preview.name"
          video
          preview
        />
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
        <span id="preview-description"
          >{{ bytes(preview.size) }} · {{ date(preview.mtime) }}</span
        ><button class="primary" @click="download(preview)">
          <ArrowDownToLine aria-hidden="true" :size="20" />{{ t("download") }}
        </button>
      </div></template
    >
  </dialog>
</template>
