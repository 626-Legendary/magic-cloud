import { describe, expect, it } from "vitest";
import { fileKind, fileNameParts } from "./file-kind";

describe("file categories", () => {
  it("handles mixed-case extensions and distinguishes office and unknown files", () => {
    for (const [name, kind] of [
      ["REPORT.PDF", "pdf"],
      ["report.DocX", "word"],
      ["sheet.XLSX", "excel"],
      ["data.CSV", "excel"],
      ["photo.PNG", "image"],
      ["movie.MP4", "video"],
      ["song.FLAC", "audio"],
      ["notes.md", "document"],
      ["archive.zip", "unknown"],
      ["README", "unknown"],
    ]) {
      expect(fileKind({ kind: "file", name }), name).toBe(kind);
    }
  });
  it("keeps folders recognizable even if their names have extensions", () => {
    expect(fileKind({ kind: "folder", name: "photos.PDF" })).toBe("folder");
  });
});

describe("display file names", () => {
  it("separates only the final extension and preserves the original spelling", () => {
    expect(fileNameParts({ kind: "file", name: "design (1).md" })).toEqual({
      stem: "design (1)",
      extension: ".md",
    });
    expect(fileNameParts({ kind: "file", name: "archive.backup.TAR" })).toEqual(
      { stem: "archive.backup", extension: ".TAR" },
    );
  });
  it("keeps folders, dotfiles and extensionless names intact", () => {
    for (const [kind, name] of [
      ["folder", "photos.2026"],
      ["file", ".env"],
      ["file", "README"],
      ["file", "draft."],
    ] as const)
      expect(fileNameParts({ kind, name })).toEqual({
        stem: name,
        extension: "",
      });
  });
});
