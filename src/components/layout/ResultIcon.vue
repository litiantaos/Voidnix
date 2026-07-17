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
  moduleIcon?: string
}>()

const icon = computed(
  () => props.item.icon || (props.item.data?.icon as string | undefined) || props.moduleIcon,
)
const isIconFont = computed(() => icon.value?.startsWith('i-') ?? false)
const isImageIcon = computed(() => !!icon.value && !isIconFont.value)
const isModuleItem = computed(() => props.item.data?.kind === 'module')
const isFileOrFolder = computed(
  () => props.item.data?.kind === 'file' || props.item.data?.kind === 'folder',
)
const iconSrc = computed(() => {
  const i = icon.value
  return i?.startsWith('data:') ? i : 'data:image/png;base64,' + i
})
const fileIcon = computed(() => getFileIcon(props.item))

/** 字体图标类名：file/folder 无显式 icon 时按扩展名类型映射；否则用综合 icon */
const displayIcon = computed(() => {
  if (isFileOrFolder.value && !icon.value) return fileIcon.value.icon
  return icon.value
})
/** 字体图标色：扩展类用主色、file/folder 无显式 icon 用类型映射色、其余中性灰 */
const displayColor = computed(() => {
  if (isModuleItem.value) return 'text-accent'
  if (isFileOrFolder.value && !icon.value) return fileIcon.value.color
  return 'text-muted'
})
</script>
