<template>
  <div ref="listRef" flex="~ col">
    <BaseList :items="filtered" v-model:selected-index="selectedIndex">
      <template #item="{ item, selected }">
        <div
          flex
          gap="2"
          text="xs"
          leading="5"
          px="3"
          py="0.5"
          font="mono"
          rounded="md"
          :class="selected ? 'bg-black/5' : ''"
        >
          <span shrink="0" w="12" flex justify="start" :class="levelColor(item.type)"
            >[{{ item.type }}]</span
          >
          <span flex="1" break="all" text="tx-secondary">{{ item.payload }}</span>
        </div>
      </template>
    </BaseList>
    <BaseEmptyState v-if="filtered.length === 0" icon="i-ri-file-list-3-line" title="无日志" />
  </div>
</template>

<script setup lang="ts">
import {
  ref,
  nextTick,
  computed,
  onMounted,
  onActivated,
  onDeactivated,
  onUnmounted,
  watch,
} from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

interface LogFrame {
  type: string
  payload: string
}

const MAX_LOGS = 500
const appStore = useAppStore()
const logs = ref<LogFrame[]>([])
const selectedIndex = ref(0)
const listRef = ref<HTMLElement | null>(null)
/// 贴底滚动依赖的真正滚动容器（ContentView 的 scrollContainer）：View 根移除 overflow 后，
/// listRef 仅作 closest 起点，实际滚动由 scrollContainer 承担（与 BaseList 键盘滚动一致）
let scroller: HTMLElement | null = null
let channel: Channel<LogFrame> | null = null
let unlistenEnabled: (() => void) | null = null
/// 用户是否贴底（决定新日志是否自动滚到底；用户上滚查历史时不打断）
let stickToBottom = true

const filtered = computed(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return logs.value
  return logs.value.filter(
    (l) => (l.payload || '').toLowerCase().includes(q) || (l.type || '').toLowerCase().includes(q),
  )
})

function levelColor(t: string): string {
  switch (t) {
    case 'error':
      return 'text-red-500'
    case 'warning':
      return 'text-yellow-600'
    case 'debug':
      return 'text-tx-faint'
    default:
      return 'text-tx-muted'
  }
}

function openStream() {
  if (channel) return // 已开（onMounted + onActivated 首次都会触发）
  channel = new Channel<LogFrame>()
  channel.onmessage = (frame) => {
    logs.value.push(frame)
    if (logs.value.length > MAX_LOGS) logs.value.splice(0, logs.value.length - MAX_LOGS)
    if (stickToBottom) nextTick(scrollToBottom)
  }
  // level=debug：全级别推送（取消下拉筛选，统一由搜索过滤）
  invoke(CMD.proxyLogsStream, { level: 'debug', onEvent: channel }).catch(() => {})
}

function closeStream() {
  if (channel) {
    invoke(CMD.proxyStopStream, { id: 'logs' }).catch(() => {})
    channel = null
  }
}

function scrollToBottom() {
  if (scroller) scroller.scrollTop = scroller.scrollHeight
}

function onScroll() {
  if (!scroller) return
  stickToBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 24
}

/// 日志条数变化时，贴底则自动滚到底（用户上滚查历史时不打断）
watch(
  () => filtered.value.length,
  () => {
    if (stickToBottom) nextTick(scrollToBottom)
  },
)

onMounted(async () => {
  scroller =
    (listRef.value?.closest('.overflow-y-auto, .overflow-auto') as HTMLElement | null) ?? null
  scroller?.addEventListener('scroll', onScroll, { passive: true })
  openStream()
  // 代理（重新）开启/重连时清空旧日志（上次运行的记录不保留）
  unlistenEnabled = await listen<boolean>('proxy-enabled', (e) => {
    if (e.payload) logs.value = []
  })
})

// KeepAlive 缓存子视图：切走走 onDeactivated（非 onUnmounted），必须在此停流，
// 否则 /logs WS 在用户不看时持续空转、缓冲徒增。
onActivated(openStream)
onDeactivated(closeStream)

onUnmounted(() => {
  unlistenEnabled?.()
  scroller?.removeEventListener('scroll', onScroll)
  closeStream()
})
</script>
