<script setup lang="ts">
import { computed } from "vue";
import { bytes, date, type Node } from "./api";
import { fileKind, fileNameParts } from "./file-kind";
import { locale, t } from "./i18n";
const props = defineProps<{
  node: Node;
  showName?: boolean;
  titleId?: string;
}>();
const typeLabel = computed(() => {
  const extension = fileNameParts(props.node).extension.slice(1).toUpperCase();
  const label = t(`fileType_${fileKind(props.node)}`);
  return extension && !label.toUpperCase().startsWith(extension)
    ? `${extension} · ${label}`
    : label;
});
const exactSize = computed(
  () =>
    `${new Intl.NumberFormat(locale.value === "zh" ? "zh-CN" : "en-US").format(props.node.size)} ${t("bytesUnit")}`,
);
</script>
<template>
  <dl class="file-metadata">
    <div v-if="showName">
      <dt>{{ t("name") }}</dt>
      <dd>
        <h3 :id="titleId">{{ node.name }}</h3>
      </dd>
    </div>
    <div>
      <dt>{{ t("type") }}</dt>
      <dd>{{ typeLabel }}</dd>
    </div>
    <div>
      <dt>{{ t("size") }}</dt>
      <dd>
        {{ bytes(node.size) }}<small>{{ exactSize }}</small>
      </dd>
    </div>
    <div>
      <dt>{{ t("uploadedAt") }}</dt>
      <dd>
        {{ node.uploaded_at ? date(node.uploaded_at) : t("uploadTimeUnknown") }}
      </dd>
    </div>
  </dl>
</template>
