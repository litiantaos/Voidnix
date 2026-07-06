<template>
  <!-- Cmd+Enter 动作面板（窗口右下角，详情 + 动作合成；capture-phase 键盘劫持 + 外点关闭） -->
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
        v-if="open"
        ref="panelRef"
        tabindex="-1"
        class="dropdown-panel outline-none max-w-96 min-w-64 bottom-4 right-4 fixed z-50"
        role="dialog"
        aria-label="项目信息"
      >
        <div text="sm tx-primary" font="medium" class="text-justify break-words" px="3" pt="3">
          {{ result?.title }}
        </div>
        <div border="b black/5" m="x-3 y-3" />
        <template v-if="metaItems.length > 0">
          <BaseDropdownItems :items="metaItems" />
          <div border="b black/5" m="x-3 t-3 b-1" />
        </template>
        <BaseDropdownItems
          :items="actionItems"
          :active-index="menuIndex"
          @select="onMenuClick"
          @hover="(i: number) => (menuIndex = i)"
        />
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { hideWindow } from '@/utils/tauri'
import { wrapIndex } from '@/utils/dom'
import BaseDropdownItems, { type PanelItem } from '@/components/ui/BaseDropdownItems.vue'
import type { SearchResult } from '@/runtime/types'

const props = defineProps<{
  results: SearchResult[]
  selectedIndex: number
}>()

interface PathMetadata {
  size: number | null
  created: string | null
  modified: string | null
  last_used: string | null
  version: string | null
}

const appStore = useAppStore()
const open = ref(false)
const data = ref<PathMetadata | null>(null)
const result = ref<SearchResult | null>(null)
const panelRef = ref<HTMLElement>()
const menuIndex = ref(-1)

// 先拉数据再开面板（mdls 仅几十毫秒），避免加载态与内容态 swap 造成开屏抖动
async function openFor(r: SearchResult) {
  result.value = r
  data.value = null
  try {
    data.value = await invoke<PathMetadata>(CMD.getPathMetadata, { path: r.data!.path })
  } catch {
    data.value = null
  }
  menuIndex.value = selectableIndices.value[0] ?? -1
  open.value = true
  nextTick(() => panelRef.value?.focus())
}

function close() {
  open.value = false
  nextTick(() => document.getElementById('main-search-input')?.focus())
}

const metaRows = computed<{ label: string; value: string }[]>(() => {
  if (!data.value) return []
  const isApp = result.value?.data?.kind === 'application'
  const r: { label: string; value: string }[] = [{ label: '大小', value: fmtSize(data.value.size) }]
  if (isApp) {
    r.push({ label: '版本', value: data.value.version || '—' })
    r.push({ label: '创建时间', value: fmtDate(data.value.created) })
    r.push({ label: '上次打开', value: fmtDate(data.value.last_used) })
  } else {
    r.push({ label: '创建时间', value: fmtDate(data.value.created) })
    r.push({ label: '修改时间', value: fmtDate(data.value.modified) })
    r.push({ label: '上次打开', value: fmtDate(data.value.last_used) })
  }
  return r
})

const actionItems = computed<PanelItem[]>(() => [
  { type: 'item', key: 'reveal', label: '在访达中打开', icon: 'i-ri-folder-open-line' },
  { type: 'item', key: 'copyPath', label: '复制路径', icon: 'i-ri-file-copy-line' },
])

const metaItems = computed<PanelItem[]>(() =>
  metaRows.value.map((r) => ({ type: 'meta', label: r.label, value: r.value })),
)

const selectableIndices = computed(() =>
  actionItems.value
    .map((it, i) => (it.type === 'item' && !it.disabled ? i : -1))
    .filter((i) => i >= 0),
)

function moveMenu(dir: 1 | -1) {
  const ids = selectableIndices.value
  if (ids.length === 0) return
  const cur = Math.max(0, ids.indexOf(menuIndex.value))
  menuIndex.value = ids[wrapIndex(cur, ids.length, dir === 1 ? 'down' : 'up')]
}

function runAction(key: string | number) {
  const path = result.value?.data?.path
  if (!path) return
  if (key === 'reveal') {
    close()
    invoke(CMD.revealInFinder, { path })
    hideWindow()
  } else if (key === 'copyPath') {
    close()
    invoke(CMD.pasteboardWriteText, { text: path })
    appStore.showStatus('已复制路径')
  }
}

function confirmMenu() {
  const item = actionItems.value[menuIndex.value]
  if (!item || item.type !== 'item' || item.disabled || !item.key) return
  runAction(item.key)
}

function onMenuClick(i: number) {
  const item = actionItems.value[i]
  if (!item || item.type !== 'item' || item.disabled || !item.key) return
  runAction(item.key)
}

function fmtSize(bytes: number | null): string {
  if (!bytes || bytes <= 0) return '—'
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = bytes / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`
}

// mdls 日期 "2024-01-01 12:00:00 +0000" → 转 ISO 后格式化为本地 "2024-01-01 12:00"
function fmtDate(s: string | null | undefined): string {
  if (!s) return '—'
  const parts = s.split(' ')
  const iso = parts.length >= 3 ? `${parts[0]}T${parts[1]}${parts[2]}` : s
  const d = new Date(iso)
  if (isNaN(d.getTime())) return s
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function onDocKey(e: KeyboardEvent) {
  if (e.isComposing) return
  if (open.value) {
    if (e.key === 'Escape' || (e.key === 'Enter' && e.metaKey)) {
      e.preventDefault()
      e.stopPropagation()
      close()
    } else if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      confirmMenu()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      e.stopPropagation()
      moveMenu(1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      e.stopPropagation()
      moveMenu(-1)
    }
    return
  }
  // Cmd+Enter 打开：仅全局模式 + 选中项为应用/文件/文件夹且有 path
  if (e.key === 'Enter' && e.metaKey && !appStore.activeModuleId && !appStore.isDialogOpen) {
    const r = props.results[props.selectedIndex]
    const kind = r?.data?.kind
    if (r?.data?.path && (kind === 'application' || kind === 'file' || kind === 'folder')) {
      e.preventDefault()
      e.stopPropagation()
      openFor(r)
    }
  }
}

function onDocMouseDown(e: MouseEvent) {
  if (open.value && panelRef.value && !panelRef.value.contains(e.target as Node)) {
    close()
  }
}

onMounted(() => {
  document.addEventListener('keydown', onDocKey, true)
  document.addEventListener('mousedown', onDocMouseDown)
})

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onDocKey, true)
  document.removeEventListener('mousedown', onDocMouseDown)
})
</script>
