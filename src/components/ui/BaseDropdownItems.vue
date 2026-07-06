<template>
  <template v-for="(item, i) in items" :key="item.key ?? i">
    <div v-if="item.type === 'divider'" border="t black/5" m="y-1" />
    <div v-else-if="item.type === 'header'" class="group-header">{{ item.label }}</div>
    <div
      v-else-if="item.type === 'meta'"
      flex
      items="center"
      justify="between"
      gap="4"
      px="3"
      py="1"
    >
      <span text="xs tx-subtle" shrink="0">{{ item.label }}</span>
      <span text="xs tx-primary" font="medium" class="text-right min-w-0 truncate">{{
        item.value
      }}</span>
    </div>
    <div v-else @click="onSelect(i)" @mouseenter="onHover(i)" :class="rowClass(item, i)">
      <i v-if="item.icon" :class="item.icon" text="sm" />
      <span truncate>{{ item.label }}</span>
    </div>
  </template>
</template>

<script setup lang="ts">
defineOptions({ inheritAttrs: false })

export interface PanelItem {
  key?: string | number
  label?: string
  value?: string
  icon?: string
  type?: 'item' | 'header' | 'divider' | 'meta'
  disabled?: boolean
  danger?: boolean
}

const props = withDefaults(
  defineProps<{
    items: PanelItem[]
    activeIndex?: number
  }>(),
  { activeIndex: -1 },
)

const emit = defineEmits<{
  select: [index: number]
  hover: [index: number]
}>()

function rowClass(item: PanelItem, i: number) {
  const active = i === props.activeIndex && !item.disabled
  return [
    'flex items-center gap-2 text-sm font-medium px-3 py-1.5 rounded-md transition-colors truncate',
    item.disabled ? 'opacity-40 cursor-not-allowed' : '',
    active ? 'ui-active' : '',
    item.danger ? 'text-red-500' : active ? 'text-accent' : 'text-tx-secondary',
  ]
}

function onSelect(i: number) {
  if (props.items[i]?.disabled) return
  emit('select', i)
}

function onHover(i: number) {
  if (props.items[i]?.disabled) return
  emit('hover', i)
}
</script>
