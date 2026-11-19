<script setup lang="ts">
import { computed, useId, type Component } from "vue";
import { Music, AudioLines, Image, Video } from "lucide-vue-next";
import { fileKind, fileNameParts, type FileKind } from "./file-kind";
import type { Node as FileNode } from "./api";
const props = defineProps<{
  node: Pick<FileNode, "kind" | "name">;
  compact?: boolean;
}>();
const kind = computed(() => fileKind(props.node));
const mediaIcons: Partial<Record<FileKind, Component>> = {
  image: Image,
  video: Video,
  audio: Music,
};
const mediaIcon = computed(() => mediaIcons[kind.value]);
const badge = computed(() =>
  kind.value === "unknown"
    ? "FILE"
    : (fileNameParts(props.node).extension.slice(1) || "FILE")
        .toUpperCase()
        .slice(0, 8),
);
const gradientId = useId();
</script>

<template>
  <svg
    v-if="kind === 'folder'"
    class="folder-artwork"
    :class="[kind, { 'is-compact': compact }]"
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
    :is="mediaIcon"
    v-else-if="compact && mediaIcon"
    :class="kind"
    :style="{ color: `var(--file-${kind})` }"
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
    :style="{ color: `var(--file-${kind})` }"
    aria-hidden="true"
  >
    <component :is="mediaIcon" :size="48" :stroke-width="2.2" />
  </div>
  <!-- Every document context renders this same vector; compact only changes its size. -->
  <svg
    v-else
    class="document-artwork"
    :class="[kind, { 'is-compact': compact }]"
    :style="{ color: `var(--file-${kind})` }"
    viewBox="0 0 80 88"
    fill="none"
    stroke="currentColor"
    stroke-width="3.5"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path
      d="M20 5h25l17 17v50a6 6 0 0 1-6 6H20a6 6 0 0 1-6-6V11a6 6 0 0 1 6-6Z"
      fill="var(--surface)"
    />
    <path d="M45 5v13a4 4 0 0 0 4 4h13M25 35h15M25 46h25M25 57h15" />
    <g class="document-badge">
      <rect
        x="23"
        y="62"
        width="54"
        height="22"
        rx="4"
        fill="var(--surface)"
        stroke-width="2"
      />
      <text
        x="50"
        y="73"
        text-anchor="middle"
        dominant-baseline="central"
        fill="currentColor"
        stroke="none"
        :font-size="badge.length > 5 ? 9 : 14"
        font-weight="600"
      >
        {{ badge }}
      </text>
    </g>
  </svg>
</template>
