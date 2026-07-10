<template>
  <div flex="~ col">
    <BaseList :items="filtered" v-model:selected-index="selectedIndex">
      <template #item="{ item }">
        <div flex p="3" gap="3" text="xs" select="none">
          <span shrink="0" w="10" flex justify="start">
            <span class="radius-ctrl" bg="black/5" text="muted" p="x-1.5">{{
              item.metadata.network
            }}</span>
          </span>
          <span flex="1" min-w="0" truncate :title="displayHost(item)">{{
            displayHost(item)
          }}</span>
          <span shrink="0" w="28" truncate text="secondary" :title="chainText(item)">{{
            chainText(item)
          }}</span>
          <div shrink="0" w="28" flex justify="end" items="center" gap="3" text="muted">
            <span tabular="nums" whitespace="nowrap">↑ {{ fmtTraffic(item.upload) }}</span>
            <span tabular="nums" whitespace="nowrap">↓ {{ fmtTraffic(item.download) }}</span>
          </div>
        </div>
      </template>
    </BaseList>
    <BaseEmptyState v-if="filtered.length === 0" icon="i-ri-links-line" title="无活动连接" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, onDeactivated, onUnmounted } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { formatBytes } from '@/utils/format'
import BaseList from '@/components/ui/BaseList.vue'

const fmtTraffic = (n: number) => formatBytes(n, { compact: true })
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

interface ConnMeta {
  network: string
  type: string
  host: string
  destinationIP: string
  destinationPort: string
  processPath: string
}
interface Conn {
  id: string
  metadata: ConnMeta
  upload: number
  download: number
  chains: string[]
}
interface ConnFrame {
  connections: Conn[]
}

const appStore = useAppStore()
const conns = ref<Conn[]>([])
const selectedIndex = ref(0)
let channel: Channel<ConnFrame> | null = null

const filtered = computed(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return conns.value
  return conns.value.filter((c) => {
    const m = c.metadata
    return (
      m.host.toLowerCase().includes(q) ||
      m.processPath.toLowerCase().includes(q) ||
      m.destinationIP.toLowerCase().includes(q) ||
      displayHost(c).toLowerCase().includes(q) ||
      chainText(c).toLowerCase().includes(q)
    )
  })
})

function displayHost(c: Conn): string {
  const m = c.metadata
  if (m.host) return m.host
  if (m.destinationIP) return `${m.destinationIP}:${m.destinationPort}`
  return '—'
}

function chainText(c: Conn): string {
  return c.chains?.[0] ?? '—'
}

function openStream() {
  if (channel) return // 已开（onMounted + onActivated 首次都会触发）
  channel = new Channel<ConnFrame>()
  channel.onmessage = (frame) => {
    conns.value = frame.connections ?? []
  }
  invoke(CMD.proxyConnectionsStream, { onEvent: channel }).catch(() => {
    /* 静默：mihomo 未运行时后端不 spawn，前端显示空 */
  })
}

function closeStream() {
  if (!channel) return
  invoke(CMD.proxyStopStream, { id: 'connections' }).catch(() => {})
  channel = null
}

// KeepAlive 缓存子视图：切走走 onDeactivated（非 onUnmounted），必须在此停流，
// 否则 /connections WS 在用户不看时持续空转（2Hz 全量快照）。
onMounted(openStream)
onActivated(openStream)
onDeactivated(closeStream)
onUnmounted(closeStream)
</script>
