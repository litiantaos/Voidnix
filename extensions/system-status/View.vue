<template>
  <!-- 顶距交给 CHROME_HEIGHT，与列表 px-3 pb-3 同构，勿 p-t 叠双层 -->
  <div p="x-3 b-3" flex="~ col" gap="3">
    <BaseEmptyState
      v-if="error"
      icon="i-ri-error-warning-line"
      :title="error"
      description="请检查系统权限或重启应用"
    />

    <template v-else-if="staticInfo && snapshot">
      <!-- 顶部设备概览 -->
      <section p="3" class="soft-card" flex flex-wrap gap="1.5" items="center">
        <span flex shrink="0" gap="2" items="center" mr="2">
          <i class="i-ri-computer-line text-xs text-secondary" />
          <span text="xs secondary" font="medium">设备</span>
        </span>
        <span
          text="xs secondary"
          font="medium"
          truncate
          class="cursor-pointer hover:text-primary"
          title="点击复制机型"
          @click="copyField(resolveModel(staticInfo.model), '已复制机型')"
        >
          {{ resolveModel(staticInfo.model) }}
        </span>
        <span text="muted">·</span>
        <span text="xs muted" shrink="0">
          {{ staticInfo.os_name }} {{ staticInfo.os_version }}
        </span>
        <span text="muted">·</span>
        <span
          text="xs muted"
          shrink="0"
          class="cursor-pointer hover:text-primary"
          title="点击复制主机名"
          @click="copyField(staticInfo.hostname, '已复制主机名')"
        >
          {{ staticInfo.hostname }}
        </span>
        <span text="muted">·</span>
        <span text="xs muted" shrink="0">运行 {{ formatUptime(snapshot.uptime) }}</span>
        <span text="muted">·</span>
        <span text="xs muted" shrink="0" tabular-nums title="负载均值 1 / 5 / 15 分钟">
          负载 {{ snapshot.load_one.toFixed(2) }} / {{ snapshot.load_five.toFixed(2) }} /
          {{ snapshot.load_fifteen.toFixed(2) }}
        </span>
        <template v-if="snapshot.thermal !== 'nominal'">
          <span text="muted">·</span>
          <span
            text="xs"
            shrink="0"
            :class="thermalClass(snapshot.thermal)"
            :title="thermalTitle(snapshot.thermal)"
          >
            {{ thermalText(snapshot.thermal) }}
          </span>
        </template>
        <template v-if="snapshot.low_power_mode">
          <span text="muted">·</span>
          <span text="xs warning" shrink="0">低电量模式</span>
        </template>
      </section>

      <!-- 第 1 行：CPU + Memory -->
      <div gap="3" grid="~ cols-2">
        <!-- CPU -->
        <section p="3" class="soft-card">
          <div flex gap="2" items="center" leading="none">
            <i class="i-ri-cpu-line text-xs text-secondary" />
            <span text="xs secondary" font="medium">处理器</span>
          </div>
          <div mb="1.5" mt="2" flex gap="2" items="baseline">
            <span text="lg primary" font="semibold" tabular-nums>
              {{ snapshot.cpu_usage.toFixed(1) }}<span text="xs muted">%</span>
            </span>
            <span v-if="snapshot.cpu_temp !== null" text="xs muted" ml="auto" tabular-nums>
              {{ snapshot.cpu_temp.toFixed(0) }}°C
            </span>
          </div>
          <div mb="2" rounded="full" class="fill-active" h="1.5" overflow="hidden">
            <div
              rounded="full"
              h="full"
              class="transition-all duration-300 ease-out"
              :class="usageColor(snapshot.cpu_usage)"
              :style="{ width: `${clamp(snapshot.cpu_usage)}%` }"
            />
          </div>
          <div mb="2" flex gap="0.5" h="3" items="end">
            <div
              v-for="(u, i) in snapshot.cpu_cores_usage"
              :key="i"
              rounded="sm"
              class="fill-active"
              flex
              flex-1
              h="full"
              items="end"
              overflow="hidden"
            >
              <div
                w="full"
                class="transition-all duration-300 ease-out"
                :class="usageColor(u)"
                :style="{ height: `${clamp(u)}%` }"
              />
            </div>
          </div>
          <div text="xs muted" truncate tabular-nums>
            <span v-if="staticInfo.cpu_model">{{ staticInfo.cpu_model }} · </span
            >{{ staticInfo.cpu_cores }} 核<span v-if="gpuLabel"> · {{ gpuLabel }}</span>
          </div>
        </section>

        <!-- Memory -->
        <section p="3" class="soft-card">
          <div flex gap="2" items="center" leading="none">
            <i class="i-ri-database-2-line text-xs text-secondary" />
            <span text="xs secondary" font="medium">内存</span>
          </div>
          <div mb="1.5" mt="2" flex items="baseline" justify="between">
            <span text="lg primary" font="semibold" tabular-nums>
              {{ formatBytes(snapshot.used_memory) }}
              <span text="xs muted">/ {{ formatBytes(snapshot.total_memory) }}</span>
            </span>
            <span text="xs muted" tabular-nums>
              {{ pct(snapshot.used_memory, snapshot.total_memory).toFixed(0) }}%
            </span>
          </div>
          <div mb="2" rounded="full" class="fill-active" h="1.5" overflow="hidden">
            <div
              rounded="full"
              h="full"
              class="transition-all duration-300 ease-out"
              :class="usageColor(pct(snapshot.used_memory, snapshot.total_memory))"
              :style="{
                width: `${clamp(pct(snapshot.used_memory, snapshot.total_memory))}%`,
              }"
            />
          </div>
          <div text="xs muted" tabular-nums>可用 {{ formatBytes(snapshot.available_memory) }}</div>
          <div
            v-if="snapshot.total_swap > 0 || snapshot.used_swap > 0"
            text="xs muted"
            mt="0.5"
            tabular-nums
          >
            交换
            {{ formatBytes(snapshot.used_swap)
            }}<template v-if="snapshot.total_swap > 0">
              / {{ formatBytes(snapshot.total_swap) }}</template
            >
          </div>
        </section>
      </div>

      <!-- 第 2 行：Disk + Power -->
      <div gap="3" grid="~ cols-2">
        <!-- Disk -->
        <section p="3" class="soft-card">
          <div flex gap="2" items="center" leading="none">
            <i class="i-ri-hard-drive-3-line text-xs text-secondary" />
            <span text="xs secondary" font="medium">磁盘</span>
            <span v-if="diskFsLabel" text="xs muted" ml="auto" shrink="0" tabular-nums>{{
              diskFsLabel
            }}</span>
          </div>
          <div v-for="d in snapshot.disks_usage" :key="d.mount_point" mb="1.5" mt="2" last:mb="0">
            <div mb="1" flex items="baseline" justify="between">
              <span text="xs secondary" font="medium" truncate min-w="0">
                {{ d.name || d.mount_point }}
                <span v-if="d.kind && d.kind !== 'Unknown'" text="muted"> · {{ d.kind }}</span>
                <span v-if="d.removable" text="muted"> · 外置</span>
              </span>
              <span text="xs muted" ml="2" shrink="0" tabular-nums>
                {{ formatBytes(d.used) }} / {{ formatBytes(d.total) }}
              </span>
            </div>
            <div rounded="full" class="fill-active" h="1.5" overflow="hidden">
              <div
                rounded="full"
                h="full"
                class="transition-all duration-300 ease-out"
                :class="usageColor(pct(d.used, d.total))"
                :style="{ width: `${clamp(pct(d.used, d.total))}%` }"
              />
            </div>
          </div>
        </section>

        <!-- Power -->
        <section p="3" class="soft-card">
          <div flex gap="2" items="center" leading="none">
            <i
              v-if="snapshot.battery"
              :class="batteryIcon(snapshot.battery)"
              class="text-xs text-secondary"
            />
            <i v-else class="i-ri-plug-line text-xs text-secondary" />
            <span text="xs secondary" font="medium">电池</span>
            <span v-if="snapshot.battery" text="xs muted" ml="auto" shrink="0">
              {{ batteryStateText(snapshot.battery.state) }}
            </span>
          </div>
          <template v-if="snapshot.battery">
            <div mb="1.5" mt="2" flex items="baseline" justify="between">
              <span text="lg primary" font="semibold" tabular-nums>
                {{ snapshot.battery.level }}<span text="xs muted">%</span>
              </span>
              <span v-if="snapshot.battery.health !== null" text="xs muted" tabular-nums>
                健康 {{ snapshot.battery.health }}%
              </span>
            </div>
            <div mb="2" rounded="full" class="fill-active" h="1.5" overflow="hidden">
              <div
                rounded="full"
                h="full"
                class="transition-all duration-300 ease-out"
                :class="batteryColor(snapshot.battery.level)"
                :style="{ width: `${snapshot.battery.level}%` }"
              />
            </div>
            <div text="xs muted" flex flex-wrap gap="1.5" tabular-nums>
              <span v-if="snapshot.battery.time_to_empty !== null">
                剩余 {{ formatDuration(snapshot.battery.time_to_empty) }}
              </span>
              <span v-if="snapshot.battery.time_to_full !== null">
                充满 {{ formatDuration(snapshot.battery.time_to_full) }}
              </span>
              <span v-if="snapshot.battery.adapter_watts !== null">
                {{ snapshot.battery.adapter_watts }}W
              </span>
              <span v-if="snapshot.battery.cycles !== null">
                {{ snapshot.battery.cycles }} 循环
              </span>
              <span v-if="snapshot.battery.temperature !== null">
                {{ snapshot.battery.temperature.toFixed(1) }}°C
              </span>
            </div>
          </template>
          <div v-else text="xs muted" mt="2">无电池（台式机）</div>
        </section>
      </div>

      <!-- 第 3 行：Processes + Network -->
      <div gap="3" grid="~ cols-2">
        <!-- Processes -->
        <section p="3" class="soft-card">
          <div flex gap="2" items="center" leading="none">
            <i class="i-ri-apps-2-line text-xs text-secondary" />
            <span text="xs secondary" font="medium">进程</span>
          </div>
          <div v-for="(p, i) in snapshot.processes" :key="i" mt="2" flex gap="2" items="center">
            <span text="xs muted" shrink="0" w="3" tabular-nums>{{ i + 1 }}</span>
            <div flex-1 min-w="0">
              <div mb="0.5" flex items="baseline" justify="between">
                <span text="xs secondary" font="medium" truncate>{{ p.name }}</span>
                <span text="xs muted" ml="2" shrink="0" tabular-nums>
                  {{ p.cpu.toFixed(1) }}% · {{ formatBytes(p.memory) }}
                </span>
              </div>
              <div rounded="full" class="fill-active" h="0.5" overflow="hidden">
                <div
                  rounded="full"
                  h="full"
                  class="transition-all duration-300 ease-out"
                  :class="usageColor(p.cpu)"
                  :style="{ width: `${clamp(p.cpu)}%` }"
                />
              </div>
            </div>
          </div>
        </section>

        <!-- Network -->
        <section p="3" class="soft-card">
          <div flex gap="2" items="center" leading="none">
            <i class="i-ri-signal-tower-line text-xs text-secondary" />
            <span text="xs secondary" font="medium">网络</span>
          </div>
          <div mb="1.5" mt="2" flex gap="2" items="center">
            <i class="i-ri-arrow-down-line text-xs text-muted" />
            <div flex flex-1 gap="0.5" h="3" items="end">
              <div
                v-for="(v, i) in sparkData(downHistory)"
                :key="'d' + i"
                rounded="xs"
                bg="accent"
                opacity="70"
                flex-1
                min-w="0.5"
                class="transition-all duration-300"
                :style="{ height: `${sparkHeight(v, downHistory)}%` }"
              />
            </div>
            <span text="xs secondary" font="medium" ml="1" shrink="0" tabular-nums>
              {{ formatRate(snapshot.net_down) }}
            </span>
          </div>
          <div mb="1.5" flex gap="2" items="center">
            <i class="i-ri-arrow-up-line text-xs text-muted" />
            <div flex flex-1 gap="0.5" h="3" items="end">
              <div
                v-for="(v, i) in sparkData(upHistory)"
                :key="'u' + i"
                rounded="xs"
                opacity="70"
                flex-1
                min-w="0.5"
                class="bg-warning transition-all duration-300"
                :style="{ height: `${sparkHeight(v, upHistory)}%` }"
              />
            </div>
            <span text="xs secondary" font="medium" ml="1" shrink="0" tabular-nums>
              {{ formatRate(snapshot.net_up) }}
            </span>
          </div>
          <div text="xs muted" flex gap="1.5" truncate items="center">
            <span text="muted" shrink="0">内网</span>
            <span
              truncate
              tabular-nums
              class="cursor-pointer hover:text-primary"
              title="点击复制内网 IP"
              @click="copyField(snapshot.local_ip, '已复制内网 IP')"
            >
              {{ snapshot.local_ip || '—' }}
            </span>
          </div>
        </section>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, onDeactivated, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { isTauri } from '@/utils/tauri'
import { writeText } from '@/utils/clipboard'
import { formatBytes, formatRate } from '@/utils/format'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

interface DiskStatic {
  name: string
  mount_point: string
  fs_type: string
  kind: string
  removable: boolean
  total: number
}
interface SystemStaticInfo {
  hostname: string
  os_name: string
  os_version: string
  model: string
  cpu_model: string
  cpu_cores: number
  gpu_model: string
  gpu_cores: number | null
  total_memory: number
  disks: DiskStatic[]
}
interface DiskUsage {
  name: string
  mount_point: string
  kind: string
  removable: boolean
  used: number
  total: number
}
interface ProcessInfo {
  name: string
  cpu: number
  memory: number
}
interface BatteryInfo {
  level: number
  state: string
  cycles: number | null
  health: number | null
  time_to_empty: number | null
  time_to_full: number | null
  adapter_watts: number | null
  temperature: number | null
}
interface SystemSnapshot {
  cpu_usage: number
  cpu_cores_usage: number[]
  cpu_temp: number | null
  used_memory: number
  available_memory: number
  total_memory: number
  used_swap: number
  total_swap: number
  load_one: number
  load_five: number
  load_fifteen: number
  thermal: string
  low_power_mode: boolean
  disks_usage: DiskUsage[]
  battery: BatteryInfo | null
  net_up: number
  net_down: number
  local_ip: string
  uptime: number
  processes: ProcessInfo[]
}

const staticInfo = ref<SystemStaticInfo | null>(null)
const snapshot = ref<SystemSnapshot | null>(null)
const error = ref('')
const downHistory = ref<number[]>([])
const upHistory = ref<number[]>([])

let timer: number | undefined
let polling = false
let activated = false
let windowFocused = false
let unlistenFocus: (() => void) | undefined
const POLL_INTERVAL = 2000
const HISTORY_LEN = 16

async function fetchStatic() {
  if (!isTauri) return
  try {
    staticInfo.value = await invoke<SystemStaticInfo>(CMD.systemStaticInfo)
  } catch (e) {
    error.value = `读取系统信息失败：${e ?? '未知错误'}`
  }
}

async function fetchSnapshot() {
  if (!isTauri || polling) return
  polling = true
  try {
    const s = await invoke<SystemSnapshot>(CMD.systemSnapshot)
    snapshot.value = s
    downHistory.value = [...downHistory.value, s.net_down].slice(-HISTORY_LEN)
    upHistory.value = [...upHistory.value, s.net_up].slice(-HISTORY_LEN)
  } catch (e) {
    console.error('[system-status] snapshot failed:', e)
  } finally {
    polling = false
  }
}

function startPolling() {
  stopPolling()
  fetchSnapshot()
  timer = window.setInterval(fetchSnapshot, POLL_INTERVAL)
}

function stopPolling() {
  if (timer !== undefined) {
    clearInterval(timer)
    timer = undefined
  }
}

// 双门控：组件激活（KeepAlive）AND 窗口可见（聚焦）才轮询。
// 窗口隐藏时 onDeactivated 不触发（仅切模块触发），需监听 focus 避免后台持续轮询。
function syncPolling() {
  if (activated && windowFocused) startPolling()
  else stopPolling()
}

onMounted(() => {
  fetchStatic()
  if (isTauri) {
    windowFocused = true
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        windowFocused = focused
        syncPolling()
      })
      .then((fn) => {
        unlistenFocus = fn
      })
  }
})
onActivated(() => {
  activated = true
  syncPolling()
})
onDeactivated(() => {
  activated = false
  syncPolling()
})
onBeforeUnmount(() => {
  stopPolling()
  unlistenFocus?.()
})

// ── 派生显示 ──

const diskFsLabel = computed(() => {
  if (!staticInfo.value?.disks.length) return ''
  const fsTypes = new Set(
    staticInfo.value.disks
      .map((d) => d.fs_type)
      .filter(Boolean)
      .map((s) => s.toUpperCase()),
  )
  return [...fsTypes].join(' / ')
})

// GPU：Apple Silicon 型号与 CPU 重复，仅显示核数；Intel/AMD 独显显示型号
const gpuLabel = computed(() => {
  const g = staticInfo.value
  if (!g) return ''
  if (g.gpu_cores !== null) return `GPU ${g.gpu_cores} 核`
  if (g.gpu_model) return `GPU ${g.gpu_model}`
  return ''
})

// ── 交互 ──

async function copyField(value: string, label = '已复制') {
  if (!value) return
  try {
    await writeText(value)
    useAppStore().showStatus(label, { duration: 800 })
  } catch (e) {
    console.error('[system-status] copy failed:', e)
  }
}

function thermalText(state: string): string {
  return (
    {
      fair: '轻微发热',
      serious: '热节流',
      critical: '严重过热',
    }[state] ?? state
  )
}

function thermalTitle(state: string): string {
  return (
    {
      fair: '系统热状态：Fair',
      serious: '系统热状态：Serious',
      critical: '系统热状态：Critical',
    }[state] ?? state
  )
}

function thermalClass(state: string): string {
  if (state === 'critical' || state === 'serious') return 'text-danger'
  if (state === 'fair') return 'text-warning'
  return 'text-muted'
}

// ── 格式化 ──

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d > 0) return `${d}d ${h}h`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

function formatDuration(mins: number): string {
  const h = Math.floor(mins / 60)
  const m = mins % 60
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}`
  return `${m}m`
}

function pct(used: number, total: number): number {
  if (total <= 0) return 0
  return (used / total) * 100
}

function clamp(n: number): number {
  return Math.max(0, Math.min(100, n))
}

function usageColor(p: number): string {
  if (p >= 85) return 'bg-danger'
  if (p >= 60) return 'bg-warning'
  return 'bg-accent'
}

function batteryColor(level: number): string {
  if (level <= 20) return 'bg-danger'
  if (level <= 40) return 'bg-warning'
  return 'bg-accent'
}

function batteryIcon(b: BatteryInfo): string {
  if (b.state === 'charging') return 'i-ri-battery-charge-line'
  if (b.level >= 90) return 'i-ri-battery-line'
  return 'i-ri-battery-low-line'
}

function batteryStateText(state: string): string {
  return { charging: '充电中', discharging: '使用中', full: '已充满' }[state] ?? state
}

// 网络波形：历史数据不足时补占位，使波形条稳定显示
function sparkData(data: number[]): number[] {
  if (data.length >= HISTORY_LEN) return data
  const pad = HISTORY_LEN - data.length
  return [...new Array(pad).fill(0), ...data]
}

function sparkHeight(v: number, data: number[]): number {
  const max = Math.max(...data, 1)
  return Math.max(8, max > 0 ? (v / max) * 100 : 0)
}

// ── 机型代码 → 友好名（前缀匹配，未识别返回原代码）──
const MODEL_PREFIXES: [string, string][] = [
  ['iMac21', 'iMac 24" (M1, 2021)'],
  ['iMac24', 'iMac 24" (M3, 2023)'],
  ['Macmini9', 'Mac mini (M1, 2020)'],
  ['Macmini14', 'Mac mini (M2, 2023)'],
  ['Macmini15', 'Mac mini (M2 Pro, 2023)'],
  ['Macmini16', 'Mac mini (M4, 2024)'],
  ['Macmini17', 'Mac mini (M4 Pro, 2024)'],
  ['Mac13,1', 'Mac Studio (M1 Max, 2022)'],
  ['Mac13,2', 'Mac Studio (M1 Ultra, 2022)'],
  ['Mac14,13', 'Mac Studio (M2 Max, 2023)'],
  ['Mac14,14', 'Mac Studio (M2 Ultra, 2023)'],
  ['MacPro7', 'Mac Pro (2019)'],
  ['Mac14,8', 'Mac Pro (M2 Ultra, 2023)'],
  ['MacBookAir10', 'MacBook Air (M1, 2020)'],
  ['MacBookAir8', 'MacBook Air (Retina, 2018-2019)'],
  ['MacBookAir9', 'MacBook Air (Retina, 2020)'],
  ['Mac14,2', 'MacBook Air 13" (M2, 2022)'],
  ['Mac15,12', 'MacBook Air 15" (M2, 2023)'],
  ['Mac14,15', 'MacBook Air 13" (M3, 2024)'],
  ['Mac15,13', 'MacBook Air 15" (M3, 2024)'],
  ['MacBookPro17', 'MacBook Pro 13" (M1, 2020)'],
  ['MacBookPro14', 'MacBook Pro 13" (Intel, 2016-2020)'],
  ['MacBookPro15', 'MacBook Pro 15"/16" (Intel, 2018-2021)'],
  ['MacBookPro16', 'MacBook Pro 16" (Intel, 2019-2021)'],
  ['Mac14,4', 'MacBook Pro 13" (M2, 2022)'],
  ['Mac14,5', 'MacBook Pro 14" (M1 Pro/Max, 2021)'],
  ['Mac14,6', 'MacBook Pro 16" (M1 Pro/Max, 2021)'],
  ['Mac15,3', 'MacBook Pro 14" (M2 Pro/Max, 2023)'],
  ['Mac15,6', 'MacBook Pro 16" (M2 Pro/Max, 2023)'],
  ['Mac15,7', 'MacBook Pro 14" (M3, 2023)'],
  ['Mac15,8', 'MacBook Pro 14" (M3 Pro/Max, 2023)'],
  ['Mac15,9', 'MacBook Pro 16" (M3 Pro/Max, 2023)'],
  ['Mac15,10', 'MacBook Pro 14" (M3 Max, 2023)'],
  ['Mac15,11', 'MacBook Pro 16" (M3 Max, 2023)'],
  ['Mac16,1', 'MacBook Pro 14" (M4, 2024)'],
  ['Mac16,2', 'MacBook Pro 14" (M4 Pro, 2024)'],
  ['Mac16,3', 'MacBook Pro 16" (M4 Pro, 2024)'],
  ['Mac16,5', 'MacBook Pro 16" (M4 Max, 2024)'],
  ['Mac16,6', 'MacBook Pro 14" (M4 Max, 2024)'],
  ['Mac16,7', 'MacBook Pro 16" (M4 Max, 2024)'],
  ['Mac16,8', 'MacBook Pro 14" (M4 Max, 2024)'],
]

function resolveModel(code: string): string {
  for (const [prefix, name] of MODEL_PREFIXES) {
    if (code.startsWith(prefix)) return name
  }
  return code || 'Mac'
}
</script>
