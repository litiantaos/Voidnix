<template>
  <!-- 纯图标渲染：背景由 ResultItem iconWrapper 统一（图片=透明、字体/file=fill-mist）
       file/folder 无显式 icon 按扩展名映射色，其他字体图标 text-muted -->
  <img
    v-if="isImageIcon"
    :src="iconSrc"
    h="[115%]"
    max-w="[115%]"
    w="[115%]"
    object="contain"
    :class="{ rounded: item.data?.iconStyle === 'rounded' }"
    :alt="item.title"
  />
  <i v-else-if="displayIcon" :class="[displayIcon, displayColor, 'text-sm']" />
  <span v-else text="sm muted" font="medium">
    {{ item.title[0]?.toUpperCase() }}
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { SearchResult } from '@/runtime/types'
import { getFileIcon } from '@/utils/icons'

const props = defineProps<{
  item: SearchResult
  extensionIcon?: string
}>()

/** item 自身声明的图标（app 图标 / 扩展结果图标），不含 extensionIcon 兜底 */
const itemIcon = computed(() => props.item.icon || (props.item.data?.icon as string | undefined))
const isFileOrFolder = computed(
  () => props.item.data?.kind === 'file' || props.item.data?.kind === 'folder',
)
const fileIcon = computed(() => getFileIcon(props.item))

/** 综合图标优先级：item 显式 > file/folder 类型映射 > extension 兜底 */
const icon = computed(() => {
  if (itemIcon.value) return itemIcon.value
  if (isFileOrFolder.value) return fileIcon.value.icon
  return props.extensionIcon
})
const isIconFont = computed(() => icon.value?.startsWith('i-') ?? false)
const isImageIcon = computed(() => !!icon.value && !isIconFont.value)
const isExtensionItem = computed(() => props.item.data?.kind === 'extension')
const iconSrc = computed(() => {
  const i = icon.value
  return i?.startsWith('data:') ? i : 'data:image/png;base64,' + i
})

/** 字体图标类名 */
const displayIcon = computed(() => icon.value)
/** 字体图标色：扩展类用主色、file/folder 用类型映射色、其余中性灰 */
const displayColor = computed(() => {
  if (isExtensionItem.value) return 'text-accent'
  if (isFileOrFolder.value && !itemIcon.value) return fileIcon.value.color
  return 'text-muted'
})
</script>
