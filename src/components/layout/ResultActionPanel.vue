<template>
  <!-- Cmd+Enter 动作面板（窗口右下角，详情 + 动作合成） -->
  <Teleport to="body">
    <Transition name="ui-popup" appear>
      <div
        v-if="open"
        ref="panelRef"
        tabindex="-1"
        class="dropdown-panel outline-none max-w-96 min-w-64 bottom-3 right-3 fixed z-50"
        role="dialog"
        :aria-label="t('action.itemInfo')"
      >
        <div text="sm primary" font="medium" class="text-justify break-words" px="3" pt="3">
          {{ result?.title }}
        </div>
        <div class="border-b border-divider" m="x-3 y-3" />
        <template v-if="metaItems.length > 0">
          <BaseDropdownItems :items="metaItems" />
          <div class="border-b border-divider" m="x-3 t-3 b-1" />
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
import { t } from '@/runtime/i18n'
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
  { type: 'item', key: 'reveal', label: t('action.openInFinder'), icon: 'i-ri-folder-open-line' },
  { type: 'item', key: 'copyPath', label: t('action.copyPath'), icon: 'i-ri-file-copy-line' },
])

const metaRows = computed<{ label: string; value: string }[]>(() => {
  if (!data.value) return []
  const isApp = result.value?.data?.kind === 'application'
  const r: { label: string; value: string }[] = [
    { label: t('action.size'), value: formatBytes(data.value.size, { empty: '—' }) },
  ]
  if (isApp) {
    r.push({ label: t('action.version'), value: data.value.version || '—' })
  }
  r.push(
    { label: t('action.created'), value: fmtDate(data.value.created) },
    { label: t('action.modified'), value: fmtDate(data.value.modified) },
    { label: t('action.lastOpened'), value: fmtDate(data.value.last_used) },
  )
  return r
})

const metaItems = computed<PanelItem[]>(() =>
  metaRows.value.map((r) => ({ type: 'meta', label: r.label, value: r.value })),
)

let pendingResult: SearchResult | null = null

const { open, menuIndex, close, toggleOpen, onMenuClick } = useActionPanel({
  panelRef,
  getItems: () => actionItems.value,
  onSelect: runAction,
  canOpen: () => {
    if (appStore.activeExtId || appStore.isDialogOpen) return false
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
    appStore.showStatus(t('action.copiedPath'))
  }
}

/// 右键入口（经 MainView 转发）：暴露 composable 统一的 toggle（已开则关，否则 canOpen → openFor）
defineExpose({ toggleOpen })

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
