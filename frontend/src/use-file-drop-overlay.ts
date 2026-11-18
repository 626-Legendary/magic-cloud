import { onMounted, onUnmounted, ref, watch } from "vue";

export function useFileDropOverlay(enabled: () => boolean) {
  const active = ref(false);
  let area: HTMLElement | undefined;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  function reset() {
    clearTimeout(idleTimer);
    idleTimer = undefined;
    area = undefined;
    active.value = false;
  }
  function dragOver(event: DragEvent) {
    if (!enabled() || !event.dataTransfer?.types.includes("Files")) {
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
    if (event.key === "Escape") reset();
  }
  function hidden() {
    if (document.hidden) reset();
  }
  watch(enabled, (value) => {
    if (!value) reset();
  });
  onMounted(() => {
    window.addEventListener("dragover", outsideDrag, true);
    window.addEventListener("drop", reset, true);
    window.addEventListener("dragend", reset, true);
    window.addEventListener("blur", reset);
    window.addEventListener("pointerdown", reset, true);
    window.addEventListener("keydown", cancelKey, true);
    document.addEventListener("visibilitychange", hidden);
  });
  onUnmounted(() => {
    reset();
    window.removeEventListener("dragover", outsideDrag, true);
    window.removeEventListener("drop", reset, true);
    window.removeEventListener("dragend", reset, true);
    window.removeEventListener("blur", reset);
    window.removeEventListener("pointerdown", reset, true);
    window.removeEventListener("keydown", cancelKey, true);
    document.removeEventListener("visibilitychange", hidden);
  });
  return { active, dragOver, dragLeave, reset };
}
