import type { Node } from "./api";

export function fileKind(node: Pick<Node, "kind" | "name">) {
  if (node.kind === "folder") return "folder";
  const ext = node.name.split(".").pop()?.toLowerCase() || "";
  if (/^(png|jpe?g|gif|webp|avif|bmp|svg)$/.test(ext)) return "image";
  if (/^(mp4|mkv|mov|webm|avi|m4v)$/.test(ext)) return "video";
  if (/^(mp3|wav|ogg|flac|m4a|aac)$/.test(ext)) return "audio";
  if (ext === "pdf") return "pdf";
  if (/^(docx?|odt|rtf)$/.test(ext)) return "word";
  if (/^(xlsx?|ods|csv)$/.test(ext)) return "excel";
  if (/^(txt|md|rs|js|ts|json|py|html|css)$/.test(ext)) return "document";
  return "unknown";
}

export function fileNameParts(node: Pick<Node, "kind" | "name">) {
  const dot = node.name.lastIndexOf(".");
  if (node.kind === "folder" || dot <= 0 || dot === node.name.length - 1)
    return { stem: node.name, extension: "" };
  return { stem: node.name.slice(0, dot), extension: node.name.slice(dot) };
}
