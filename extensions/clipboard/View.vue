<template>
  <BaseEmptyState
    v-if="history.length === 0"
    title="暂无剪贴板记录"
    icon="i-ri-clipboard-line"
    :loading="loading"
  />

  <div v-else h="full">
    <BaseList
      :items="history"
      multi-select
      :selected-ids="selectedIds"
      :keyboard-active="!menuOpen && !previewOpen && !editOpen"
      id-field="id"
      @update:selected-ids="selectedIds = $event"
      @select="selectedIndex = $event"
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
            <img
              v-else-if="item.content_type === 'image' && imageCache.get(item.id)"
              :src="imageCache.get(item.id)"
              rounded="md"
              bg="black/5"
              border="~ black/1"
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
        </BaseListItem>
      </template>
    </BaseList>
  </div>

  <!-- Cmd+回车 动作菜单（界面右下角，同下拉框样式，键盘可达）-->
  <Teleport to="body">
    <Transition
      appear
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 translate-y-2 scale-95"
      enter-to-class="opacity-100 translate-y-0 scale-100"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100 translate-y-0 scale-100"
      leave-to-class="opacity-0 translate-y-2 scale-95"
    >
      <div
        v-if="menuOpen"
        ref="menuRef"
        tabindex="-1"
        class="dropdown-panel outline-none bottom-4 right-4 fixed z-50"
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
        <div flex items-center justify-center min-h="full" p="5">
          <img
            v-if="previewType === 'image' && previewImage"
            :src="previewImage"
            max-w="full"
            max-h="full"
            object="contain"
            rounded="md"
            alt="预览图片"
          />
          <span
            v-else-if="previewType === 'image'"
            class="i-ri-loader-4-line text-2xl text-tx-muted animate-spin"
          />
          <div v-else text="sm tx-primary" leading="relaxed" whitespace="pre-wrap" break="words">
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
  nextTick,
  onActivated,
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
import { useAppStore } from '@/stores/app'
import { wrapIndex } from '@/utils/dom'

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
  () => appStore.activeModuleId,
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
    appStore.showStatus('粘贴失败', { kind: 'error', duration: 4000 })
  }
}

// ── 动作菜单（Cmd+回车，界面右下角，键盘可达）──
const menuOpen = ref(false)
const menuIndex = ref(-1)
const menuTarget = ref<ClipboardItem | null>(null)
const menuBatch = ref(false)
const menuRef = ref<HTMLElement>()

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
    icon: item.is_favorite ? 'i-ri-star-fill text-amber-400' : 'i-ri-star-line',
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

const selectableIndices = computed(() =>
  actionMenuItems.value
    .map((it, i) => (it.type === 'item' && !it.disabled ? i : -1))
    .filter((i) => i >= 0),
)

function openMenu(item: ClipboardItem) {
  menuBatch.value = false
  menuTarget.value = item
  menuIndex.value = selectableIndices.value[0] ?? -1
  menuOpen.value = true
  nextTick(() => menuRef.value?.focus())
}

function openBatchMenu() {
  menuBatch.value = true
  menuTarget.value = null
  menuIndex.value = selectableIndices.value[0] ?? -1
  menuOpen.value = true
  nextTick(() => menuRef.value?.focus())
}

function closeMenu() {
  menuOpen.value = false
  menuBatch.value = false
  nextTick(() => document.getElementById('main-search-input')?.focus())
}

function moveMenu(dir: 1 | -1) {
  const ids = selectableIndices.value
  if (ids.length === 0) return
  const cur = Math.max(0, ids.indexOf(menuIndex.value))
  menuIndex.value = ids[wrapIndex(cur, ids.length, dir === 1 ? 'down' : 'up')]
}

function runMenuAction(key: string | number) {
  if (menuBatch.value) {
    if (key !== 'delete') return
    const ids = [...selectedIds.value]
    closeMenu()
    void deleteItems(ids)
    return
  }
  const target = menuTarget.value
  if (!target) return
  closeMenu()
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

function confirmMenu() {
  const item = actionMenuItems.value[menuIndex.value]
  if (!item || item.type !== 'item' || item.disabled || !item.key) return
  runMenuAction(item.key)
}

function onMenuClick(i: number) {
  const item = actionMenuItems.value[i]
  if (!item || item.type !== 'item' || item.disabled || !item.key) return
  runMenuAction(item.key)
}

function onDocMouseDown(e: MouseEvent) {
  if (!menuOpen.value) return
  if (menuRef.value && !menuRef.value.contains(e.target as Node)) closeMenu()
}

// 捕获相：菜单/预览打开时拦截相关键并阻断冒泡，先于全局 useResultNavigation（exitModule）执行；
// 并在菜单关闭时拦截 Cmd+回车开菜单（先于 BaseList，避免其清空多选）
function onDocKey(e: KeyboardEvent) {
  if (menuOpen.value) {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        e.stopPropagation()
        moveMenu(1)
        return
      case 'ArrowUp':
        e.preventDefault()
        e.stopPropagation()
        moveMenu(-1)
        return
      case 'Enter':
        e.preventDefault()
        e.stopPropagation()
        if (e.metaKey) closeMenu()
        else confirmMenu()
        return
      case 'Escape':
        e.preventDefault()
        e.stopPropagation()
        closeMenu()
        return
    }
    return
  }
  // Cmd+回车开菜单（多选→批量删除，否则→当前项完整菜单）。仅 clipboard 激活时拦截
  if (
    e.key === 'Enter' &&
    e.metaKey &&
    !previewOpen.value &&
    !editOpen.value &&
    appStore.activeModuleId === 'clipboard'
  ) {
    e.preventDefault()
    e.stopPropagation()
    if (selectedIds.value.size > 0) openBatchMenu()
    else {
      const item = history.value[selectedIndex.value]
      if (item) openMenu(item)
    }
    return
  }
  if (previewOpen.value && e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    previewOpen.value = false
  }
}

onMounted(() => {
  document.addEventListener('mousedown', onDocMouseDown)
  document.addEventListener('keydown', onDocKey, true)
})
onUnmounted(() => {
  document.removeEventListener('mousedown', onDocMouseDown)
  document.removeEventListener('keydown', onDocKey, true)
})

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
    kind: 'warning',
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
