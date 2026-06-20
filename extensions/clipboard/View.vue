<template>
  <BaseEmptyState
    v-if="history.length === 0"
    title="暂无剪贴板记录"
    icon="i-ri-clipboard-line"
    :loading="loading"
  />

  <BaseList
    v-else
    ref="listRef"
    :items="history"
    multi-select
    :selected-ids="selectedIds"
    id-field="id"
    @update:selected-ids="selectedIds = $event"
    @execute="handleExecute"
  >
    <template #item="{ item, selected, multiSelected }">
      <BaseListItem
        :ref="(el: unknown) => setImageRef(el, item)"
        :selected="selected || multiSelected"
        multiline-title
      >
        <template #icon>
          <div
            v-if="getColor(item)"
            rounded
            h="4"
            w="4"
            :style="{ backgroundColor: getColor(item)! }"
          ></div>
          <i
            v-else-if="item.content_type === 'text'"
            class="i-ri-t-box-line text-sm text-accent"
          ></i>
          <i
            v-else-if="item.content_type === 'image'"
            class="i-ri-image-line text-sm text-emerald-500"
          ></i>
          <i v-else class="i-ri-folder-3-line text-sm text-amber-500"></i>
        </template>
        <template #title>
          <div
            v-if="item.content_type === 'text'"
            whitespace-pre-wrap
            wrap-break-word
            line-clamp="5"
          >
            {{ item.content }}
          </div>
          <div v-else-if="item.content_type === 'image'" m="b-2 t-0.5">
            <img
              v-if="imageCache.get(item.id)"
              :src="imageCache.get(item.id)"
              rounded="md"
              bg="black/5"
              h="32"
              w="48"
              object="cover top"
              loading="lazy"
              alt="剪贴板图片"
            />
            <div v-else rounded="md" bg="black/5" flex h="32" w="48" class="flex-center">
              <div class="i-ri-image-line text-2xl text-tx-faint"></div>
            </div>
          </div>
          <div v-else truncate>
            {{ item.content.split('/').filter(Boolean).pop() || item.content }}
          </div>
        </template>
        <template #subtitle>
          <div flex gap="2" items="center">
            <span>{{ item.source_app }}</span>
            <span>•</span>
            <span>{{ formatTime(item.created_at) }}</span>
            <template v-if="item.file_size">
              <span>•</span>
              <span>{{ formatSize(item.file_size) }}</span>
            </template>
            <template v-if="item.image_width && item.image_height">
              <span>•</span>
              <span>{{ item.image_width }}×{{ item.image_height }}</span>
            </template>
          </div>
        </template>
        <template #trailing>
          <BaseButton
            variant="ghost"
            :icon="
              item.is_favorite ? 'i-ri-star-fill text-amber-400' : 'i-ri-star-line text-tx-subtle'
            "
            @click.stop="toggleFavorite(item.id)"
          />
        </template>
      </BaseListItem>
    </template>
  </BaseList>
</template>

<script setup lang="ts">
import { ref, onActivated, onDeactivated, onUnmounted, watch, shallowReactive } from 'vue'
import {
  history,
  activeTab,
  loading,
  fetchClipboardHistory,
  invalidateCache,
  registerDeleteHandler,
  triggerDelete,
} from './index'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { useAppStore } from '@/stores/app'
import { onKeyStroke } from '@/composables/events'
import { isComposing as isComposingCheck } from '@/utils/dom'

const appStore = useAppStore()

const listRef = ref<{ selectedIndex: number; setSelectedIndex: (i: number) => void }>()
const selectedIds = ref(new Set<string>())

let debounceTimer: ReturnType<typeof setTimeout>
watch([activeTab, () => appStore.searchQuery], ([tab, query]) => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    fetchClipboardHistory(query, tab === 'favorites')
  }, 80)
})

watch(
  () => appStore.activeModuleId,
  (id) => {
    if (id !== 'clipboard') {
      clearTimeout(debounceTimer)
      selectedIds.value = new Set()
    }
  },
)

async function handleExecute() {
  const ids =
    selectedIds.value.size > 0
      ? [...selectedIds.value]
      : [history.value[listRef.value?.selectedIndex ?? 0]?.id].filter(Boolean)
  selectedIds.value = new Set()
  if (ids.length === 0) return
  try {
    if (ids.length > 1) {
      await invoke(CMD.pasteClipboardItems, { ids })
    } else {
      await invoke(CMD.pasteClipboardItem, { id: ids[0] })
    }
    invalidateCache()
  } catch (e) {
    console.error('Failed to paste clipboard:', e)
  }
}

const toggleFavorite = async (id: string) => {
  try {
    await invoke(CMD.toggleClipboardFavorite, { id })
    history.value = history.value.map((i) =>
      i.id === id ? { ...i, is_favorite: !i.is_favorite } : i,
    )
    if (activeTab.value === 'favorites') {
      history.value = history.value.filter((i) => i.is_favorite)
    }
    invalidateCache()
  } catch (e) {
    console.error('Failed to toggle favorite:', e)
  }
}

async function handleDelete() {
  const ids =
    selectedIds.value.size > 0
      ? [...selectedIds.value]
      : [history.value[listRef.value?.selectedIndex ?? 0]?.id].filter(Boolean)
  if (ids.length === 0) return

  const count = ids.length
  const confirmed = await appStore.showConfirm({
    title: '删除剪贴板记录',
    message: count > 1 ? `确定要删除 ${count} 条记录吗？` : '确定要删除这条记录吗？',
    kind: 'warning',
    okLabel: '删除',
    cancelLabel: '取消',
  })
  if (!confirmed) return

  try {
    await invoke(CMD.deleteClipboardItems, { ids })
    selectedIds.value = new Set()
    invalidateCache()
    await fetchClipboardHistory(appStore.searchQuery, activeTab.value === 'favorites')
  } catch (e) {
    console.error('Failed to delete clipboard items:', e)
  }
}

onActivated(() => {
  registerDeleteHandler(handleDelete)
  // 重置到「全部」标签并重新过滤展示。不 invalidateCache：setup 注册的
  // clipboard-updated 监听器已实时保活缓存，进入时重查 DB 属冗余（命中缓存分支同步过滤）。
  activeTab.value = 'all'
  fetchClipboardHistory('', false)
})
onDeactivated(() => registerDeleteHandler(() => {}))

onKeyStroke('Backspace', (e) => {
  if (appStore.activeModuleId !== 'clipboard') return
  if (appStore.activeSubview) return
  if (!(e.metaKey || e.ctrlKey)) return
  if (appStore.isComposing || isComposingCheck(e)) return
  e.preventDefault()
  triggerDelete()
})

// ── 图片懒加载 ──
const imageCache = shallowReactive(new Map<string, string>())
const pendingImages = new Set<string>()

const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue
      const id = (entry.target as HTMLElement).dataset.imageId
      if (id && !imageCache.has(id) && !pendingImages.has(id)) {
        pendingImages.add(id)
        invoke<string | null>(CMD.getClipboardImage, { id }).then((data) => {
          if (data) imageCache.set(id, data)
          pendingImages.delete(id)
        })
      }
      observer.unobserve(entry.target)
    }
  },
  { rootMargin: '200px' },
)

function setImageRef(el: unknown, item: { id: string; content_type: string }) {
  if (item.content_type !== 'image') return
  if (imageCache.has(item.id) || pendingImages.has(item.id)) return
  const htmlEl = (el as { $el?: HTMLElement })?.$el ?? (el as HTMLElement | null)
  if (!htmlEl) return
  htmlEl.dataset.imageId = item.id
  observer.observe(htmlEl)
}

onUnmounted(() => observer.disconnect())

// ── 工具函数 ──
const COLOR_RE = /^(?:#[0-9a-fA-F]{3,8}|(?:rgb|hsl)a?\s*\([\d\s,%.\/]+\))$/
const colorCache = new Map<string, string | null>()

function getColor(item: { id: string; content_type: string; content: string }): string | null {
  if (item.content_type !== 'text') return null
  let cached = colorCache.get(item.id)
  if (cached === undefined) {
    const line = item.content.trim().split('\n')[0].trim()
    cached = COLOR_RE.test(line) ? line : null
    colorCache.set(item.id, cached)
  }
  return cached
}

function formatTime(at: string): string {
  const m = at.match(/\d{2}:\d{2}/)
  return m ? m[0] : at
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
</script>
