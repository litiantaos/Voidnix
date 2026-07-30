<template>
  <BaseEmptyState
    v-if="history.length === 0"
    title="暂无剪贴板记录"
    icon="i-ri-clipboard-line"
    :loading="loading"
  />

  <div v-else>
    <BaseList
      :items="history"
      multi-select
      :selected-ids="selectedIds"
      :keyboard-active="!open && !previewOpen && !editOpen"
      id-field="id"
      @update:selected-ids="selectedIds = $event"
      @select="selectedIndex = $event"
      @execute="handleExecute"
    >
      <template #item="{ item }">
        <BaseListItem :ref="(el: unknown) => setImageRef(el, item)" multiline-title>
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
              class="i-ri-image-line text-sm text-success"
            ></i>
            <i v-else class="i-ri-folder-3-line text-sm text-warning"></i>
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
            <img
              v-else-if="item.content_type === 'image' && imageCache.get(item.id)"
              :src="imageCache.get(item.id)"
              class="border border-divider radius-ctrl border-solid fill-ctrl"
              h="32"
              w="48"
              object="cover top"
              loading="lazy"
              alt="剪贴板图片"
            />
            <div v-else truncate>
              {{ item.content.split('/').filter(Boolean).pop() || item.content }}
            </div>
          </template>
          <template #subtitle>
            <div flex gap="1.5" items="center">
              <span>{{ item.source_app }}</span>
              <span text="muted">·</span>
              <span>{{ formatTime(item.created_at) }}</span>
              <template v-if="item.file_size">
                <span text="muted">·</span>
                <span>{{ formatBytes(item.file_size) }}</span>
              </template>
              <template v-if="item.image_width && item.image_height">
                <span text="muted">·</span>
                <span>{{ item.image_width }}×{{ item.image_height }}</span>
              </template>
            </div>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>

  <!-- Cmd+回车 动作菜单（界面右下角，同下拉框样式，键盘可达）-->
  <Teleport to="body">
    <Transition name="ui-popup" appear>
      <div
        v-if="open"
        ref="panelRef"
        tabindex="-1"
        class="dropdown-panel outline-none bottom-3 right-3 fixed z-50"
        role="menu"
      >
        <BaseDropdownItems
          :items="actionMenuItems"
          :active-index="menuIndex"
          @select="onMenuClick"
          @hover="(i: number) => (menuIndex = i)"
        />
      </div>
    </Transition>
  </Teleport>

  <!-- 预览覆盖层（Esc 关闭）-->
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="previewOpen"
        class="hide-scrollbar"
        z="100"
        inset-0
        fixed
        bg="surface"
        overflow="auto"
      >
        <div flex items-center justify-center min-h="full" p="3">
          <img
            v-if="previewType === 'image' && previewImage"
            :src="previewImage"
            max-w="full"
            max-h="full"
            object="contain"
            class="radius-ctrl"
            alt="预览图片"
          />
          <span
            v-else-if="previewType === 'image'"
            class="i-ri-loader-4-line text-2xl text-muted animate-spin"
          />
          <div v-else text="sm primary" leading="relaxed" whitespace="pre-wrap" break="words">
            {{ previewText }}
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>

  <!-- 编辑弹窗（仅文本）-->
  <BaseDialog
    v-if="editOpen"
    title="编辑文本"
    variant="form"
    size="md"
    show-footer
    ok-label="保存"
    @confirm="saveEdit"
    @cancel="editOpen = false"
  >
    <div class="form-field">
      <BaseTextarea
        v-model="editText"
        :rows="12"
        :max-height="0"
        :auto-resize="false"
        :submit-on-enter="false"
        placeholder="编辑剪贴板文本"
      />
    </div>
  </BaseDialog>
</template>

<script setup lang="ts">
import {
  ref,
  computed,
  onActivated,
  onDeactivated,
  onMounted,
  onUnmounted,
  watch,
  shallowReactive,
} from 'vue'
import {
  history,
  activeTab,
  activeType,
  loading,
  fetchClipboardHistory,
  invalidateCache,
} from './index'
import type { ClipboardItem } from './index'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseDropdownItems, { type PanelItem } from '@/components/ui/BaseDropdownItems.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import { useActionPanel } from '@/composables/useActionPanel'
import { useAppStore } from '@/stores/app'
import { formatBytes, toErrorMessage } from '@/utils/format'

const appStore = useAppStore()

const selectedIds = ref(new Set<string>())
const selectedIndex = ref(0)

let debounceTimer: ReturnType<typeof setTimeout>
watch([activeTab, activeType, () => appStore.searchQuery], ([tab, , query]) => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    fetchClipboardHistory(query, tab === 'favorites')
  }, 80)
})

watch(
  () => appStore.activeExtId,
  (id) => {
    if (id !== 'clipboard') {
      clearTimeout(debounceTimer)
      selectedIds.value = new Set()
    }
  },
)

async function handleExecute(item: ClipboardItem, _index: number, _e?: KeyboardEvent) {
  // Cmd+回车开菜单由捕获相监听拦截（避免 BaseList 清空多选）；此处仅处理粘贴（回车/双击）
  const ids = selectedIds.value.size > 0 ? [...selectedIds.value] : [item.id].filter(Boolean)
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
    appStore.showStatus(toErrorMessage(e, '粘贴失败'), { kind: 'error', duration: 4000 })
  }
}

// ── 动作菜单（Cmd+回车，界面右下角，键盘可达）──
// 键盘导航 / 外点关闭 / Cmd+Enter 打开拦截 由 useActionPanel 统一承载
const panelRef = ref<HTMLElement>()
const menuTarget = ref<ClipboardItem | null>(null)
const menuBatch = ref(false)

const actionMenuItems = computed<PanelItem[]>(() => {
  if (menuBatch.value) {
    return [
      {
        type: 'item',
        key: 'delete',
        label: `删除 ${selectedIds.value.size} 条`,
        icon: 'i-ri-delete-bin-line',
        danger: true,
      },
    ]
  }
  const item = menuTarget.value
  if (!item) return []
  const isText = item.content_type === 'text'
  const items: PanelItem[] = []
  if (item.content_type !== 'file') {
    items.push({ type: 'item', key: 'preview', label: '预览', icon: 'i-ri-eye-line' })
  }
  items.push({
    type: 'item',
    key: 'favorite',
    label: item.is_favorite ? '取消收藏' : '收藏',
    icon: item.is_favorite ? 'i-ri-star-fill text-warning' : 'i-ri-star-line',
  })
  if (isText) {
    items.push({ type: 'item', key: 'edit', label: '编辑', icon: 'i-ri-edit-line' })
  }
  items.push({
    type: 'item',
    key: 'delete',
    label: '删除',
    icon: 'i-ri-delete-bin-line',
    danger: true,
  })
  return items
})

const { open, menuIndex, close, onMenuClick } = useActionPanel({
  panelRef,
  getItems: () => actionMenuItems.value,
  onSelect: runMenuAction,
  shouldOpen: (e) => {
    if (e.isComposing) return false
    if (appStore.activeExtId !== 'clipboard') return false
    if (previewOpen.value || editOpen.value) return false
    // 多选 → 批量删除菜单；否则 → 当前项完整菜单
    if (selectedIds.value.size > 0) {
      menuBatch.value = true
      menuTarget.value = null
      return true
    }
    const item = history.value[selectedIndex.value]
    if (!item) return false
    menuBatch.value = false
    menuTarget.value = item
    return true
  },
})

function runMenuAction(key: string | number) {
  if (menuBatch.value) {
    if (key !== 'delete') return
    const ids = [...selectedIds.value]
    close()
    void deleteItems(ids)
    return
  }
  const target = menuTarget.value
  if (!target) return
  close()
  switch (key) {
    case 'favorite':
      void toggleFavorite(target.id)
      break
    case 'preview':
      void openPreview(target)
      break
    case 'edit':
      void openEdit(target)
      break
    case 'delete':
      void deleteItems([target.id])
      break
  }
}

// 预览覆盖层 Esc 关闭（独立捕获相监听；preview 与菜单互斥，菜单由 composable 处理）
function onPreviewKey(e: KeyboardEvent) {
  if (!previewOpen.value) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopImmediatePropagation()
    previewOpen.value = false
  }
}
onMounted(() => document.addEventListener('keydown', onPreviewKey, true))
onUnmounted(() => document.removeEventListener('keydown', onPreviewKey, true))

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

// ── 预览 ──
const previewOpen = ref(false)
const previewType = ref<'text' | 'image'>('text')
const previewText = ref('')
const previewImage = ref('')

async function openPreview(item: ClipboardItem) {
  if (item.content_type === 'image') {
    previewType.value = 'image'
    previewImage.value = ''
    previewOpen.value = true
    try {
      const data = await invoke<string | null>(CMD.getClipboardImage, { id: item.id })
      if (data) previewImage.value = data
    } catch (e) {
      console.error('Failed to load image:', e)
    }
    return
  }
  previewType.value = 'text'
  previewText.value = '加载中…'
  previewOpen.value = true
  try {
    previewText.value = (await invoke<string | null>(CMD.getClipboardText, { id: item.id })) ?? ''
  } catch (e) {
    console.error('Failed to load text:', e)
    previewText.value = '加载失败'
  }
}

// ── 编辑（仅文本）──
const editOpen = ref(false)
const editText = ref('')
const editingId = ref('')

async function openEdit(item: ClipboardItem) {
  editingId.value = item.id
  editText.value = '加载中…'
  editOpen.value = true
  try {
    editText.value = (await invoke<string | null>(CMD.getClipboardText, { id: item.id })) ?? ''
  } catch (e) {
    console.error('Failed to load text:', e)
    editText.value = ''
  }
}

async function saveEdit() {
  if (!editingId.value) return
  try {
    await invoke(CMD.updateClipboardText, { id: editingId.value, content: editText.value })
    invalidateCache()
    await fetchClipboardHistory(appStore.searchQuery, activeTab.value === 'favorites')
  } catch (e) {
    console.error('Failed to update text:', e)
  }
  editOpen.value = false
}

// ── 删除 ──
async function deleteItems(ids: string[]) {
  if (ids.length === 0) return
  const confirmed = await appStore.showConfirm({
    title: '删除剪贴板记录',
    message: ids.length > 1 ? `确定要删除 ${ids.length} 条记录吗？` : '确定要删除这条记录吗？',
    okLabel: '删除',
    cancelLabel: '取消',
  })
  if (!confirmed) return
  try {
    await invoke(CMD.deleteClipboardItems, { ids })
    invalidateCache()
    selectedIds.value = new Set()
    await fetchClipboardHistory(appStore.searchQuery, activeTab.value === 'favorites')
  } catch (e) {
    console.error('Failed to delete clipboard item:', e)
  }
}

onActivated(() => {
  activeTab.value = 'all'
  activeType.value = 'all'
  fetchClipboardHistory('', false)
})

onDeactivated(() => {
  // 离开扩展时清空颜色缓存，避免长期驻留增长（重新进入时自然重建）
  colorCache.clear()
})

// ── 图片懒加载 ──
const IMAGE_CACHE_MAX = 30
const imageCache = shallowReactive(new Map<string, string>())
const pendingImages = new Set<string>()

function cacheImage(id: string, data: string) {
  if (imageCache.size >= IMAGE_CACHE_MAX) {
    const first = imageCache.keys().next().value
    if (first) imageCache.delete(first)
  }
  imageCache.set(id, data)
}

const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue
      const id = (entry.target as HTMLElement).dataset.imageId
      if (id && !imageCache.has(id) && !pendingImages.has(id)) {
        pendingImages.add(id)
        invoke<string | null>(CMD.getClipboardImage, { id }).then((data) => {
          if (data) cacheImage(id, data)
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
  const date = at.slice(0, 10)
  const now = new Date()
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  if (date === today) {
    const m = at.match(/\d{2}:\d{2}/)
    return m ? m[0] : at
  }
  return date.slice(5)
}
</script>
