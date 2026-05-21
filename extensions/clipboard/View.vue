<template>
  <BaseEmptyState
    v-if="history.length === 0 && !loading"
    title="暂无剪贴板记录"
    icon="i-ri-clipboard-line"
  />

  <BaseList
    v-else
    :items="history"
    :keyboard-navigation="true"
    @execute="(item) => copyToClipboard(item.id)"
  >
    <template #item="{ item, selected, setRef, execute }">
      <BaseListItem
        :ref="setRef"
        :selected="selected"
        :multilineTitle="shouldMultiline(item)"
        :icon="
          item.content_type === 'image'
            ? 'i-ri-image-fill'
            : item.content_type === 'file'
              ? 'i-ri-file-fill'
              : 'i-ri-file-text-fill'
        "
        @click="execute"
      >
        <template #title>
          <div
            v-if="item.content_type === 'text'"
            class="whitespace-pre-wrap wrap-break-word line-clamp-5"
          >
            {{ item.content }}
          </div>
          <div v-else-if="item.content_type === 'image'" class="mb-2 mt-0.5">
            <img
              :src="item.content"
              class="rounded-md bg-black/5 max-h-32 max-w-full object-contain"
              loading="lazy"
              alt="剪贴板图片"
            />
          </div>
          <div v-else class="truncate">
            {{ '[文件] ' + item.content.split('/').pop() }}
          </div>
        </template>
        <template #subtitle>
          <div class="flex gap-2 items-center">
            <span>{{ item.source_app }}</span>
            <span>•</span>
            <span>{{ item.created_at }}</span>
          </div>
        </template>
        <template #trailing>
          <button
            class="p-1.5 rounded-md transition-all focus:outline-none hover:bg-black/5"
            :class="!item.is_favorite && 'opacity-50 group-hover:opacity-100'"
            @click="toggleFavorite(item.id, $event)"
          >
            <div
              :class="
                item.is_favorite
                  ? 'i-ri-star-fill text-yellow-400'
                  : 'i-ri-star-line text-tx-subtle'
              "
              class="text-sm"
            ></div>
          </button>
        </template>
      </BaseListItem>
    </template>
  </BaseList>
</template>

<script setup lang="ts">
import { onMounted, watch } from 'vue'
import { history, activeTab, fetchClipboardHistory, invalidateCache, loading } from './index'
import { invoke } from '@tauri-apps/api/core'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import { useAppStore } from '@/stores/app'

const appStore = useAppStore()

const MULTILINE_THRESHOLD = 80
const shouldMultiline = (item: { content_type: string; content: string }) =>
  item.content_type === 'image' ||
  (item.content_type === 'text' && item.content.length > MULTILINE_THRESHOLD)

onMounted(() => {
  activeTab.value = 'all'
  fetchClipboardHistory(appStore.searchQuery, false)
})

let debounceTimer: ReturnType<typeof setTimeout>
watch([activeTab, () => appStore.searchQuery], ([tab, query]) => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    fetchClipboardHistory(query, tab === 'favorites')
  }, 80)
})

const copyToClipboard = async (id: string) => {
  try {
    await invoke('paste_clipboard_item', { id })
  } catch (e) {
    console.error('Failed to paste clipboard:', e)
  }
}

const toggleFavorite = async (id: string, event: MouseEvent) => {
  event.stopPropagation()
  try {
    await invoke('toggle_clipboard_favorite', { id })
    const item = history.value.find((i) => i.id === id)
    if (item) {
      history.value = history.value.map((i) =>
        i.id === id ? { ...i, is_favorite: !i.is_favorite } : i,
      )
    }
    if (activeTab.value === 'favorites') {
      history.value = history.value.filter((i) => i.is_favorite)
    }
    invalidateCache()
  } catch (e) {
    console.error('Failed to toggle favorite:', e)
  }
}
</script>