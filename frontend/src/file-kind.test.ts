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
      ["archive.zip", "archive"],
      ["README", "unknown"],
      ["favicon.ICO", "image"],
      ["clip.FLV", "video"],
      ["clip.WMV", "video"],
      ["sound.WMA", "audio"],
      ["score.MID", "audio"],
      ["report.PAGES", "word"],
      ["report.WPS", "word"],
      ["sheet.NUMBERS", "excel"],
      ["sheet.ET", "excel"],
      ["slides.PPTX", "presentation"],
      ["slides.KEY", "presentation"],
      ["slides.DPS", "presentation"],
      ["backup.7Z", "archive"],
      ["disk.ISO", "archive"],
      ["installer.EXE", "windows"],
      ["installer.MSI", "windows"],
      ["installer.DMG", "macos"],
      ["installer.PKG", "macos"],
      ["installer.DEB", "linux"],
      ["installer.RPM", "linux"],
      ["installer.AppImage", "linux"],
      ["installer.APK", "android"],
      ["artwork.PSD", "design"],
      ["artwork.FIG", "design"],
      ["project.PRPROJ", "design"],
      ["component.TSX", "document"],
      ["config.YAML", "document"],
      ["source.ts", "document"],
      ["image.png.exe", "windows"],
      [".png", "unknown"],
      ["video.mp4.", "unknown"],
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
