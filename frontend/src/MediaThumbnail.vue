<script setup lang="ts">
import { ref, watch, onBeforeUnmount } from "vue";
import { Film } from "lucide-vue-next";
import { t } from "./i18n";
const props = defineProps<{
  id: string;
  name: string;
  video?: boolean;
  preview?: boolean;
}>();
const state = ref<"pending" | "ready" | "failed">("pending");
const source = ref("");
let timer: ReturnType<typeof setTimeout> | undefined;
let request: AbortController | undefined;
let generation = 0;
let retries = 0;
function cleanup() {
  clearTimeout(timer);
  request?.abort();
  if (source.value) URL.revokeObjectURL(source.value);
  source.value = "";
}
function imageFailed() {
  if (source.value) URL.revokeObjectURL(source.value);
  source.value = "";
  state.value = "failed";
}
async function check(token: number) {
  request = new AbortController();
  const controller = request;
  const timeout = setTimeout(() => controller.abort(), 15000);
  try {
    const response = await fetch(
      `/api/nodes/${encodeURIComponent(props.id)}/thumbnail`,
      { credentials: "same-origin", signal: request.signal },
    );
    if (token !== generation) return;
    if (response.status === 202) {
      await response.body?.cancel();
      timer = setTimeout(() => void check(token), 2000);
    } else if (
      response.ok &&
      response.headers.get("content-type")?.startsWith("image/")
    ) {
      const blob = await response.blob();
      if (token !== generation) return;
      source.value = URL.createObjectURL(blob);
      state.value = "ready";
    } else if (response.status >= 500 && retries++ < 3) {
      timer = setTimeout(() => void check(token), 2000);
    } else state.value = "failed";
  } catch (error) {
    if (token !== generation) return;
    if (retries++ < 3) timer = setTimeout(() => void check(token), 2000);
    else state.value = "failed";
  } finally {
    clearTimeout(timeout);
  }
}
watch(
  () => props.id,
  () => {
    generation++;
    cleanup();
    retries = 0;
    state.value = "pending";
    void check(generation);
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  generation++;
  cleanup();
});
</script>

<template>
  <div :class="preview ? 'video-preview-thumbnail' : 'media-thumbnail'">
    <div :class="{ 'video-thumbnail-frame': preview }">
      <img
        v-if="source"
        :src="source"
        :alt="preview ? name : ''"
        @error="imageFailed"
      />
      <Film v-else-if="preview" :size="48" aria-hidden="true" />
      <span
        v-if="!preview && video && state !== 'ready'"
        class="thumbnail-status"
        >{{
          t(state === "pending" ? "thumbnailPending" : "thumbnailFailedShort")
        }}</span
      >
    </div>
    <p v-if="preview" role="status">
      {{
        t(
          state === "pending"
            ? "videoThumbnailPending"
            : state === "failed"
              ? "videoThumbnailFailed"
              : "videoHint",
        )
      }}
    </p>
  </div>
</template>

<style scoped>
.media-thumbnail {
  position: absolute;
  inset: 12px;
  overflow: hidden;
  border-radius: 8px;
  z-index: 1;
}
.media-thumbnail > div {
  position: relative;
  height: 100%;
}
.media-thumbnail:has(img) {
  box-shadow: var(--mc-shadow-md);
}
.media-thumbnail img {
  position: static;
  display: block;
  border-radius: 8px;
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.thumbnail-status {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  padding: var(--mc-space-1);
  background: var(--surface);
  color: var(--muted);
  font-size: var(--mc-font-caption);
  text-align: center;
}
.video-preview-thumbnail {
  width: min(100%, 480px);
  padding: var(--mc-space-4);
  text-align: center;
}
.video-thumbnail-frame {
  aspect-ratio: 4 / 3;
  display: grid;
  place-items: center;
  color: var(--muted);
}
.video-thumbnail-frame img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}
.video-preview-thumbnail p {
  color: var(--muted);
  font-size: var(--mc-font-secondary);
  margin: var(--mc-space-4) 0 0;
}
</style>
