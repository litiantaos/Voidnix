<template>
  <BaseListItem :icon-wrapper-class="iconWrapperClass">
    <template #icon>
      <ResultIcon :item="item" :module-icon="module?.meta.icon" />
    </template>
    <template #title>
      <span :class="{ 'text-accent': item.data?.isHighlight }">{{ item.title }}</span>
    </template>
    <template #subtitle>
      <!-- file/folder：父目录分段（head 截断 + tail 不换行），优先于 description（description=path 同时供 fuzzy 评分） -->
      <template v-if="isFileOrFolder && item.data?.path">
        <span class="flex-[0_1_auto] min-w-0 truncate" :title="getParentPath(item.data.path)">
          {{ formatPathParts(getParentPath(item.data.path)).head }}
        </span>
        <span flex="none" whitespace="nowrap">
          {{ formatPathParts(getParentPath(item.data.path)).tail }}
        </span>
      </template>
      <span v-else-if="item.description" class="flex-1 min-w-0 truncate">{{
        item.description
      }}</span>
    </template>
    <template #trailing>
      <span v-if="item.source" text="xs muted" whitespace="nowrap">{{ item.source }}</span>
    </template>
  </BaseListItem>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import ResultIcon from '@/components/layout/ResultIcon.vue'
import { getParentPath, formatPathParts } from '@/utils/format'
import type { Extension, SearchResult } from '@/runtime/types'

const props = defineProps<{
  item: SearchResult
  module?: Extension | null
}>()

const isFileOrFolder = computed(
  () => props.item.data?.kind === 'file' || props.item.data?.kind === 'folder',
)

/** 图标 wrapper 背景：图片图标透明底自显；其余默认 fill-ctrl 实色衬底 */
const iconWrapperClass = computed(() => {
  const effective = props.item.icon || (props.item.data?.icon as string | undefined)
  if (effective && !effective.startsWith('i-')) return 'bg-transparent'
  return undefined
})
</script>
