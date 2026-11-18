import type { Node } from "./api";

export const fileExtensions = {
  image: ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "ico"],
  video: ["mp4", "mkv", "mov", "webm", "avi", "m4v", "flv", "wmv"],
  audio: ["mp3", "wav", "ogg", "flac", "m4a", "aac", "wma", "mid"],
  pdf: ["pdf"],
  word: ["doc", "docx", "odt", "rtf", "pages", "wps"],
  excel: ["xls", "xlsx", "ods", "csv", "numbers", "et"],
  presentation: ["ppt", "pptx", "odp", "key", "dps"],
  archive: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
  document: [
    "txt",
    "md",
    "rs",
    "js",
    "ts",
    "json",
    "py",
    "html",
    "css",
    "jsx",
    "tsx",
    "mjs",
    "cjs",
    "vue",
    "svelte",
    "yaml",
    "yml",
    "toml",
    "xml",
    "sql",
    "sh",
    "bash",
    "c",
    "h",
    "cpp",
    "hpp",
    "java",
    "go",
    "rb",
    "php",
    "swift",
    "kt",
    "log",
    "ini",
    "conf",
  ],
  windows: ["exe", "msi"],
  macos: ["dmg", "pkg"],
  linux: ["deb", "rpm", "appimage"],
  android: ["apk"],
  design: ["psd", "ai", "sketch", "fig", "xd", "aep", "prproj"],
} as const;
export type FileKind = keyof typeof fileExtensions | "folder" | "unknown";
const kindsByExtension = new Map<string, FileKind>(
  Object.entries(fileExtensions).flatMap(([kind, extensions]) =>
    extensions.map((extension) => [extension, kind as FileKind] as const),
  ),
);
export function fileKind(node: Pick<Node, "kind" | "name">): FileKind {
  if (node.kind === "folder") return "folder";
  // .ts is ambiguous: retain TypeScript classification for ordinary source files.
  const extension = fileNameParts(node).extension.slice(1).toLowerCase();
  return kindsByExtension.get(extension) || "unknown";
}
export function fileNameParts(node: Pick<Node, "kind" | "name">) {
  const dot = node.name.lastIndexOf(".");
  if (node.kind === "folder" || dot <= 0 || dot === node.name.length - 1)
    return { stem: node.name, extension: "" };
  return { stem: node.name.slice(0, dot), extension: node.name.slice(dot) };
}
