<template>
  <!-- Cmd+Enter 动作面板（窗口右下角，详情 + 动作合成） -->
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
        class="dropdown-panel outline-none max-w-96 min-w-64 bottom-3 right-3 fixed z-50"
        role="dialog"
        aria-label="项目信息"
      >
        <div text="sm primary" font="medium" class="text-justify break-words" px="3" pt="3">
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
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { hideWindow } from '@/utils/tauri'
import BaseDropdownItems, { type PanelItem } from '@/components/ui/BaseDropdownItems.vue'
import { useActionPanel } from '@/composables/useActionPanel'
import { formatBytes } from '@/utils/format'
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
const panelRef = ref<HTMLElement>()
const data = ref<PathMetadata | null>(null)
const result = ref<SearchResult | null>(null)

const actionItems = computed<PanelItem[]>(() => [
  { type: 'item', key: 'reveal', label: '在访达中打开', icon: 'i-ri-folder-open-line' },
  { type: 'item', key: 'copyPath', label: '复制路径', icon: 'i-ri-file-copy-line' },
])

const metaRows = computed<{ label: string; value: string }[]>(() => {
  if (!data.value) return []
  const isApp = result.value?.data?.kind === 'application'
  const r: { label: string; value: string }[] = [
    { label: '大小', value: formatBytes(data.value.size, { empty: '—' }) },
  ]
  if (isApp) {
    r.push({ label: '版本', value: data.value.version || '—' })
  }
  r.push(
    { label: '创建时间', value: fmtDate(data.value.created) },
    { label: '修改时间', value: fmtDate(data.value.modified) },
    { label: '上次打开', value: fmtDate(data.value.last_used) },
  )
  return r
})

const metaItems = computed<PanelItem[]>(() =>
  metaRows.value.map((r) => ({ type: 'meta', label: r.label, value: r.value })),
)

let pendingResult: SearchResult | null = null

const { open, menuIndex, close, onMenuClick } = useActionPanel({
  panelRef,
  getItems: () => actionItems.value,
  onSelect: runAction,
  shouldOpen: (e) => {
    if (appStore.activeModuleId || appStore.isDialogOpen) return false
    if (e.isComposing) return false
    const r = props.results[props.selectedIndex]
    const kind = r?.data?.kind
    if (r?.data?.path && (kind === 'application' || kind === 'file' || kind === 'folder')) {
      pendingResult = r
      return true
    }
    return false
  },
  beforeOpen: async () => {
    result.value = pendingResult
    data.value = null
    try {
      data.value = await invoke<PathMetadata>(CMD.getPathMetadata, {
        path: pendingResult!.data!.path,
      })
    } catch {
      data.value = null
    }
  },
})

function runAction(key: string | number) {
  const path = result.value?.data?.path
  if (!path) return
  close()
  if (key === 'reveal') {
    invoke(CMD.revealInFinder, { path })
    hideWindow()
  } else if (key === 'copyPath') {
    invoke(CMD.pasteboardWriteText, { text: path })
    appStore.showStatus('已复制路径')
  }
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
</script>
