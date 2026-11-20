import { onMounted, onUnmounted, ref, watch } from "vue";

export function useFileDropOverlay(enabled: () => boolean) {
  const active = ref(false);
  let area: HTMLElement | undefined;
  let internalDrag = false;
  const internalType = "application/x-magic-cloud-internal-drag";
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let dropTimer: ReturnType<typeof setTimeout> | undefined;
  function reset() {
    clearTimeout(idleTimer);
    idleTimer = undefined;
    area = undefined;
    active.value = false;
  }
  function cancel() {
    clearTimeout(dropTimer);
    dropTimer = undefined;
    internalDrag = false;
    reset();
  }
  function acceptsFileDrop(event: DragEvent) {
    const types = Array.from(event.dataTransfer?.types || []);
    return (
      enabled() &&
      !internalDrag &&
      types.includes("Files") &&
      !types.includes(internalType)
    );
  }
  function startInternalDrag(event: DragEvent) {
    clearTimeout(dropTimer);
    dropTimer = undefined;
    internalDrag = true;
    reset();
    // Image drags can contain a File too. Keep their page origin on the payload.
    try {
      event.dataTransfer?.setData(internalType, "1");
    } catch {
      /* Some browser payloads are read-only. */
    }
    queueMicrotask(() => {
      if (event.defaultPrevented) internalDrag = false;
    });
  }
  function finishDrop() {
    reset();
    // Native event listeners can run microtasks between capture and target.
    // Keep the origin guard for the entire drop event, not only capture.
    clearTimeout(dropTimer);
    dropTimer = setTimeout(() => {
      internalDrag = false;
      dropTimer = undefined;
    }, 0);
  }
  function dragOver(event: DragEvent) {
    if (!acceptsFileDrop(event)) {
      reset();
      return;
    }
    event.preventDefault();
    area = event.currentTarget as HTMLElement;
    active.value = true;
    clearTimeout(idleTimer);
    // OS file drags can be cancelled without delivering dragend to the page.
    // A live drag keeps sending dragover events, including while held still.
    idleTimer = setTimeout(reset, 1500);
  }
  function dragLeave(event: DragEvent) {
    const target = event.currentTarget as HTMLElement;
    if (
      event.relatedTarget instanceof Node &&
      target.contains(event.relatedTarget)
    )
      return;
    const rect = target.getBoundingClientRect();
    if (
      !event.relatedTarget &&
      event.clientX > rect.left &&
      event.clientX < rect.right &&
      event.clientY > rect.top &&
      event.clientY < rect.bottom
    )
      return;
    reset();
  }
  function outsideDrag(event: DragEvent) {
    if (
      active.value &&
      area &&
      event.target instanceof Node &&
      !area.contains(event.target)
    )
      reset();
  }
  function cancelKey(event: KeyboardEvent) {
    if (event.key === "Escape") cancel();
  }
  function hidden() {
    if (document.hidden) cancel();
  }
  watch(enabled, (value) => {
    if (!value) cancel();
  });
  onMounted(() => {
    window.addEventListener("dragstart", startInternalDrag, true);
    window.addEventListener("dragover", outsideDrag, true);
    window.addEventListener("drop", finishDrop, true);
    window.addEventListener("dragend", cancel, true);
    window.addEventListener("blur", cancel);
    window.addEventListener("pointerdown", cancel, true);
    window.addEventListener("keydown", cancelKey, true);
    document.addEventListener("visibilitychange", hidden);
  });
  onUnmounted(() => {
    cancel();
    window.removeEventListener("dragstart", startInternalDrag, true);
    window.removeEventListener("dragover", outsideDrag, true);
    window.removeEventListener("drop", finishDrop, true);
    window.removeEventListener("dragend", cancel, true);
    window.removeEventListener("blur", cancel);
    window.removeEventListener("pointerdown", cancel, true);
    window.removeEventListener("keydown", cancelKey, true);
    document.removeEventListener("visibilitychange", hidden);
  });
  return { active, dragOver, dragLeave, reset, acceptsFileDrop };
}
