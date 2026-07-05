<template>
  <!-- Cmd+I 信息面板（窗口右下角，同动作菜单样式；capture-phase 键盘劫持 + 外点关闭） -->
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
        p="3"
        class="dropdown-panel outline-none max-w-96 min-w-64 bottom-4 right-4 fixed z-50"
        role="dialog"
        aria-label="项目信息"
      >
        <!-- 标题即时可用（result 在 fetch 前已设）；长文件名换行不省略 -->
        <div text="sm tx-primary" font="medium" class="text-justify break-words">
          {{ result?.title }}
        </div>
        <div border="b black/5" m="y-3" />
        <div v-if="data" space-y-3>
          <div v-for="row in rows" :key="row.label" flex items="center" justify="between" gap="4">
            <span text="xs tx-subtle" shrink="0">{{ row.label }}</span>
            <span text="xs tx-primary" font="medium" class="text-right min-w-0">{{
              row.value
            }}</span>
          </div>
        </div>
        <div v-else min-h-24 class="flex-center" text="xs tx-faint">无信息</div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
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

// 先拉数据再开面板（mdls 仅几十毫秒），避免加载态与内容态 swap 造成开屏抖动
async function openFor(r: SearchResult) {
  result.value = r
  data.value = null
  try {
    data.value = await invoke<PathMetadata>(CMD.getPathMetadata, { path: r.data!.path })
  } catch {
    data.value = null
  }
  open.value = true
  nextTick(() => panelRef.value?.focus())
}

function close() {
  open.value = false
  nextTick(() => document.getElementById('main-search-input')?.focus())
}

const rows = computed(() => {
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
  // 面板打开时：Cmd+I/Esc 关闭，方向键吞掉防背后列表响应
  if (open.value) {
    if ((e.key === 'i' && e.metaKey) || e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      close()
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
      e.stopPropagation()
    }
    return
  }
  // Cmd+I 打开：仅全局模式 + 选中项为应用/文件/文件夹且有 path
  if (e.key === 'i' && e.metaKey && !appStore.activeModuleId && !appStore.isDialogOpen) {
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
