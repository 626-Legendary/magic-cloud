try {
  const mode = localStorage.getItem("mc-theme") || "system";
  document.documentElement.dataset.theme =
    mode === "system"
      ? matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : mode;
} catch {}
