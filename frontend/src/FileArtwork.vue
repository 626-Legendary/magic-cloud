<script setup lang="ts">
import { useId } from "vue";
import { Music, AudioLines } from "lucide-vue-next";
import { fileIcons } from "./file-icons";
import type { FileKind } from "./file-kind";
defineProps<{ kind: FileKind; extension: string; compact?: boolean }>();
const gradientId = useId();
</script>

<template>
  <svg
    v-if="kind === 'folder'"
    class="folder-artwork"
    :class="{ 'is-compact': compact }"
    viewBox="0 0 96 80"
    fill="none"
    aria-hidden="true"
  >
    <defs>
      <linearGradient
        :id="gradientId"
        x1="48"
        y1="22"
        x2="48"
        y2="76"
        gradientUnits="userSpaceOnUse"
      >
        <stop stop-color="#58B5FF" />
        <stop offset="1" stop-color="#0D6EFD" />
      </linearGradient>
    </defs>
    <path
      d="M8 18a8 8 0 0 1 8-8h20l10 10h34a8 8 0 0 1 8 8v36a8 8 0 0 1-8 8H16a8 8 0 0 1-8-8V18Z"
      fill="#8ACBFF"
    />
    <path
      d="M8 32a8 8 0 0 1 8-8h64a8 8 0 0 1 8 8v32a8 8 0 0 1-8 8H16a8 8 0 0 1-8-8V32Z"
      :fill="`url(#${gradientId})`"
    />
    <path
      d="M17 28h62"
      stroke="#CDEAFF"
      stroke-width="2"
      stroke-linecap="round"
      opacity=".65"
    />
  </svg>
  <component
    :is="fileIcons[kind]"
    v-else-if="compact && ['audio', 'image', 'video'].includes(kind)"
    :size="24"
    aria-hidden="true"
  />
  <div v-else-if="kind === 'audio'" class="audio-artwork" aria-hidden="true">
    <AudioLines :size="68" :stroke-width="1.8" class="audio-wave" />
    <Music :size="32" :stroke-width="2.3" />
  </div>
  <div
    v-else-if="['image', 'video'].includes(kind)"
    class="media-artwork"
    aria-hidden="true"
  >
    <component :is="fileIcons[kind]" :size="48" :stroke-width="2.2" />
  </div>
  <div
    v-else
    class="document-artwork"
    :class="{ 'is-compact': compact }"
    aria-hidden="true"
  >
    <component :is="fileIcons[kind]" :size="68" :stroke-width="1.8" />
    <span class="document-badge">{{
      kind === "unknown"
        ? "FILE"
        : (extension || "FILE").toUpperCase().slice(0, 8)
    }}</span>
  </div>
</template>
