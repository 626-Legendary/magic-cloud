import { describe, expect, it } from "vitest";
import { fileKind } from "./file-kind";

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
