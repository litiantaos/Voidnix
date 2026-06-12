<template>
  <BaseList :items="view.items" @select="(i: number) => (selectedIndex = i)" @execute="onExecute">
    <template
      #item="{ item, index, selected }: { item: ListItem; index: number; selected: boolean }"
    >
      <BaseListItem :selected="selected" :icon="item.icon" @click="onItemClick(item, index)">
        <template #title>{{ item.title }}</template>
        <template v-if="item.subtitle" #subtitle>{{ item.subtitle }}</template>
      </BaseListItem>
    </template>
  </BaseList>
  <BaseEmptyState v-if="!view.isLoading && !view.items.length" :text="view.emptyText || '无结果'" />
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { ListView, ListItem } from '@/types/declarative'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

defineProps<{ view: ListView }>()

const emit = defineEmits<{
  action: [actionId: string, payload: { item: ListItem; index: number }]
}>()

const selectedIndex = ref(0)

function onItemClick(item: ListItem, index: number) {
  selectedIndex.value = index
  onExecute(item)
}

function onExecute(item: ListItem | undefined) {
  if (!item) return
  const action = item.actions?.find((a) => a.primary) ?? item.actions?.[0]
  emit('action', action?.id ?? 'execute', { item, index: selectedIndex.value })
}
</script>
