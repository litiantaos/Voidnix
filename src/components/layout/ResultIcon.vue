<template>
  <div v-if="isIconFont && !isModuleItem" h="6" w="6" class="flex-center">
    <i :class="[icon, 'text-xl text-black/50']" />
  </div>
  <img
    v-else-if="isImageIcon && !isModuleItem"
    :src="iconSrc"
    h="115pct"
    max-w="115pct"
    w="115pct"
    object="contain"
    :class="{ rounded: item.data?.iconStyle === 'rounded' }"
    :alt="item.title"
  />
  <div
    v-else-if="isModuleItem"
    text="sm accent"
    rounded="md"
    bg="accent/10"
    h="full"
    w="full"
    class="flex-center"
  >
    <i :class="icon || 'i-ri-apps-2-line'" />
  </div>
  <div v-else-if="isFileOrFolder" rounded="md" bg="black/4" h="full" w="full" class="flex-center">
    <i :class="[fileIcon.icon, fileIcon.color]" class="text-sm" />
  </div>
  <span v-else text="sm black/30" font="medium">
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
const isIconFont = computed(() => icon.value?.startsWith('i-'))
const isImageIcon = computed(() => icon.value && !isIconFont.value)
const isModuleItem = computed(() => props.item.data?.kind === 'module')
const isFileOrFolder = computed(
  () => props.item.data?.kind === 'file' || props.item.data?.kind === 'folder',
)
const iconSrc = computed(() => {
  const i = icon.value
  return i?.startsWith('data:') ? i : 'data:image/png;base64,' + i
})
const fileIcon = computed(() => getFileIcon(props.item))
</script>
