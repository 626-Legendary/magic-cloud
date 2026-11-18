import { Folder, Image, Video, Music, FileText } from "lucide-vue-next";
import type { FileKind } from "./file-kind";

// Non-media files share a paper silhouette; FileArtwork adds their type badge.
export const fileIcons = {
  folder: Folder,
  image: Image,
  video: Video,
  audio: Music,
  pdf: FileText,
  word: FileText,
  excel: FileText,
  presentation: FileText,
  archive: FileText,
  document: FileText,
  windows: FileText,
  macos: FileText,
  linux: FileText,
  android: FileText,
  design: FileText,
  unknown: FileText,
} satisfies Record<FileKind, unknown>;
