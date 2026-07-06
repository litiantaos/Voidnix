<template>
  <div overflow="y-auto">
    <BaseList
      ref="baseListRef"
      :items="items"
      v-model:selected-index="selectedIndex"
      :group-field="(item: ListItem) => item.group"
      :group-title="(g: string) => g"
      @execute="onExecute"
    >
      <template #group-title="{ group }">
        <div flex items="center">
          <span>{{ group }}</span>
          <div flex="~ 1" />
          <BaseButton
            v-if="group === '订阅'"
            icon="i-ri-add-line"
            title="添加订阅"
            @click.stop="openCreateModal"
          />
          <div v-else-if="group === '节点'" flex gap="2">
            <BaseButton
              icon="i-ri-focus-3-line"
              :disabled="!hasSelectedNode"
              title="定位到选中节点"
              @click.stop="locateSelected"
            />
            <BaseButton
              :icon="testing ? 'i-ri-loader-4-line animate-spin' : 'i-ri-flashlight-line'"
              :disabled="testing || nodes.length === 0"
              title="全部测速"
              @click.stop="testAll"
            />
          </div>
        </div>
      </template>

      <template #item="{ item, selected, setRef }">
        <!-- 启用代理（合并内核状态） -->
        <BaseListItem
          v-if="item.type === 'enabled'"
          :ref="setRef"
          title="开启代理"
          :selected="selected"
        >
          <template #subtitle>
            <template v-if="!coreStatus.downloaded">
              {{ isDownloading ? '正在下载内核…' : '功能依赖 mihomo 内核，请先下载' }}
            </template>
            <template v-else>
              <span truncate>内核版本：mihomo {{ coreStatus.version }}</span>
              <span v-if="isEnabled && traffic" text="tx-hint" shrink="0" ml="3">·</span>
              <span
                v-if="isEnabled && traffic"
                text="tx-muted"
                shrink="0"
                ml="2"
                flex
                items="center"
                gap="2.5"
                ><span tabular="nums" whitespace="nowrap">↑ {{ formatBytes(traffic.up) }}/s</span>
                <span tabular="nums" whitespace="nowrap"
                  >↓ {{ formatBytes(traffic.down) }}/s</span
                ></span
              >
              <span v-if="coreError" text="red-500" shrink="0" ml="2">{{ coreError }}</span>
              <span v-else-if="updateInfo?.hasUpdate" text="green-500" shrink="0" ml="2"
                >有新内核 {{ updateInfo.latest
                }}{{ isEnabled ? '，请关闭代理后更新' : '，点击下载更新' }}</span
              >
            </template>
          </template>
          <template #trailing>
            <!-- 下载/更新进行中：进度按钮（disabled） -->
            <BaseButton v-if="isDownloading" class="min-w-12 tabular-nums" disabled>{{
              downloadText
            }}</BaseButton>
            <!-- 已下载：开关 + 更新入口（仅关闭代理时显示，开启时走副标题绿色提示） -->
            <div v-else-if="coreStatus.downloaded" flex gap="2">
              <BaseButton :variant="isEnabled ? 'primary' : 'default'" @click.stop="toggleEnabled">
                {{ isEnabled ? '已开启' : '已关闭' }}
              </BaseButton>
              <BaseButton v-if="isEnabled && coreError" @click.stop="reconnect">重连</BaseButton>
              <BaseButton v-if="!isEnabled && updateInfo?.hasUpdate" @click.stop="updateCore"
                >下载更新</BaseButton
              >
            </div>
            <!-- 未下载：下载入口 -->
            <BaseButton v-else @click.stop="downloadCore">下载内核</BaseButton>
          </template>
        </BaseListItem>

        <!-- 规则模式 -->
        <BaseListItem
          v-else-if="item.type === 'mode'"
          :ref="setRef"
          title="规则模式"
          subtitle="规则按分流策略，全局代理所有流量"
          :selected="selected"
        >
          <template #trailing>
            <BaseSelect
              :model-value="config.mode"
              :options="MODE_OPTIONS"
              @update:model-value="onModeChange"
            />
          </template>
        </BaseListItem>

        <!-- 订阅项 -->
        <BaseListItem
          v-else-if="item.type === 'subscription'"
          :ref="setRef"
          :title="item.sub.name || '未命名订阅'"
          :subtitle="
            item.sub.proxyCount
              ? `${item.sub.proxyCount} 节点 · ${formatTime(item.sub.updatedAt)}`
              : item.sub.url || '未配置'
          "
          :selected="selected"
        />

        <!-- 分组切换（多 selector 订阅） -->
        <BaseListItem
          v-else-if="item.type === 'groupSelector'"
          :ref="setRef"
          title="节点分组"
          subtitle="当前显示的 selector 分组"
          :selected="selected"
        >
          <template #trailing>
            <BaseSelect
              :model-value="activeGroupName"
              :options="groupOptions"
              @update:model-value="onGroupChange"
            />
          </template>
        </BaseListItem>

        <!-- 节点项 -->
        <BaseListItem v-else-if="item.type === 'node'" :ref="setRef" :selected="selected">
          <template #title>
            <span :class="item.node.selected ? 'text-accent' : ''">
              {{ item.node.name }}
            </span>
          </template>
          <template #trailing>
            <span :class="delayColor(item.node.delay)" class="text-xs font-medium">
              {{ formatDelay(item.node.delay) || '\u00A0' }}
            </span>
          </template>
        </BaseListItem>
      </template>
    </BaseList>

    <!-- 订阅编辑弹窗 -->
    <BaseDialog
      v-if="showEditModal"
      :title="isCreating ? '添加订阅' : '编辑订阅'"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveSub"
      @cancel="closeEditModal"
    >
      <div flex="~ col" gap="4">
        <div class="form-field">
          <span class="form-label">订阅名称</span>
          <BaseInput v-model="editForm.name" placeholder="默认为订阅链接域名" />
        </div>
        <div class="form-field">
          <span class="form-label">订阅链接</span>
          <BaseInput v-model="editForm.url" placeholder="订阅 URL 或 Clash YAML URL" />
        </div>
      </div>
      <template #footer-start>
        <BaseButton
          v-if="!isCreating && config.subscriptions.length > 1"
          class="text-red-500 hover:text-red-600"
          @click="confirmRemoveFromModal"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>

    <!-- 删除订阅确认 -->
    <BaseDialog
      v-if="deletingSub"
      title="删除订阅"
      size="sm"
      show-footer
      ok-label="删除"
      @confirm="doRemoveSub"
      @cancel="deletingSub = null"
    >
      <div text="sm tx-secondary">确定删除「{{ deletingSub.name || '未命名订阅' }}」？</div>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, onDeactivated, onUnmounted, watch } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import {
  type Subscription,
  config,
  MODE_OPTIONS,
  addSubscription,
  updateSubscription,
  removeSubscription,
} from './config'
import { toErrorMessage } from '@/utils/format'
import { generateRequestId } from '@/utils/id'
import {
  type ProxiesResponse,
  DELAY_TIMEOUT,
  delayColor,
  filterNodes,
  formatBytes,
  formatDelay,
  isUserSelectorGroup,
  latestDelay,
} from './logic'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import BaseInput from '@/components/ui/BaseInput.vue'

interface NodeItem {
  id: string
  name: string
  delay: number
  selected: boolean
}

/// mihomo /traffic WS 帧（上下行速率 bytes/s）。
interface TrafficFrame {
  up: number
  down: number
}

type ListItem =
  | { type: 'enabled'; group: '代理' }
  | { type: 'mode'; group: '代理' }
  | { type: 'subscription'; group: '订阅'; sub: Subscription }
  | { type: 'groupSelector'; group: '节点' }
  | { type: 'node'; group: '节点'; node: NodeItem }

/// Tauri 命令错误为 Rust `Err(String)` → 前端 reject 值即字符串；
/// toErrorMessage 仅识别 Error 实例（字符串回落"未知错误"），这里补 string 透传便于诊断。
function errText(e: unknown): string {
  return typeof e === 'string' ? e : toErrorMessage(e)
}

const appStore = useAppStore()
const isEnabled = ref(false)
const toggling = ref(false)
const proxiesData = ref<ProxiesResponse | null>(null)
const delayMap = ref<Record<string, number>>({})
const testing = ref(false)
const selectedIndex = ref(0)
const baseListRef = ref<{ reveal: (i: number) => void } | null>(null)
const coreStatus = ref<{ downloaded: boolean; version: string; downloading: boolean }>({
  downloaded: false,
  version: '',
  downloading: false,
})
const coreProgress = ref<{ received: number; total: number | null }>({ received: 0, total: null })
/// 首个进度事件是否到达：未收到事件时显示「下载中」，收到后显示具体进度
const progressStarted = ref(false)
/// 内核更新检查结果（hasUpdate=true 时副标题提示 + 显示「更新」按钮）。null=未检查/API 失败
const updateInfo = ref<{ hasUpdate: boolean; current: string; latest: string } | null>(null)
/// 健康监测异常状态（proxy-status error 事件）：非空时开启代理项显示红色提示 + 重连按钮。
const coreError = ref('')
/// 实时流量速率（开代理时经 /traffic WS 推送，开启项副标题展示）。
const traffic = ref<TrafficFrame | null>(null)
let trafficChannel: Channel<TrafficFrame> | null = null
/// 下载中状态真相源跟随 Rust DOWNLOADING 原子（重新进入界面也能正确反映）。
const isDownloading = computed(() => coreStatus.value.downloading)
let unlistenProgress: (() => void) | null = null
// Rust gunzip 完成（bin 可用）后 emit，事件驱动刷新状态，不依赖 invoke resolve 时序
let unlistenReady: (() => void) | null = null
let unlistenEnabled: (() => void) | null = null
let unlistenMode: (() => void) | null = null
let unlistenStatus: (() => void) | null = null

// 订阅编辑弹窗（agent 模型提供商模式）
const editingId = ref('')
const isCreating = ref(false)
const showEditModal = ref(false)
const editForm = ref({ name: '', url: '' })
// 订阅删除确认
const deletingSub = ref<Subscription | null>(null)

// ── 节点 ──
// 全部用户可切的 selector 分组（排除 mihomo 隐式 GLOBAL）。多分组订阅时用户可在列表内切换。
const userGroups = computed(() => {
  if (!proxiesData.value) return [] as Array<{ name: string; all?: string[]; now?: string }>
  return Object.values(proxiesData.value.proxies).filter(
    (p): p is { name: string; type: string; all?: string[]; now?: string } =>
      isUserSelectorGroup(p),
  )
})

// 用户当前选中的分组名（空 → 回退首个 user selector，再回退 GLOBAL）
const activeGroupName = ref('')

const groupOptions = computed(() => userGroups.value.map((g) => ({ label: g.name, value: g.name })))

// 当前展示的分组：用户选中 > 首个 user selector。无 selector（无订阅）返回 null——
// 不回退 GLOBAL（其 all 仅含 DIRECT/REJECT 内置策略，非真实代理节点，展示无意义）
const mainGroup = computed(() => {
  if (!proxiesData.value) return null
  const groups = userGroups.value
  if (groups.length === 0) return null
  const chosen = activeGroupName.value ? groups.find((g) => g.name === activeGroupName.value) : null
  return (chosen ?? groups[0] ?? null) as { name: string; all?: string[]; now?: string } | null
})

function onGroupChange(value: string | number) {
  activeGroupName.value = String(value)
}

// 当前选中节点名（乐观更新：切换即标记，不等 loadProxies；loadProxies 完成后清空由 g.now 接管）
const selectedNodeName = ref('')

const nodes = computed<NodeItem[]>(() => {
  const g = mainGroup.value
  if (!g?.all) return []
  const current = selectedNodeName.value || g.now
  const list = g.all.map((name) => {
    const entry = proxiesData.value?.proxies[name]
    return {
      id: name,
      name,
      delay: delayMap.value[name] ?? latestDelay(entry?.history),
      selected: current === name,
    }
  })
  return filterNodes(list, appStore.searchQuery)
})

/// 是否存在选中节点（定位按钮 disabled 判断）
const hasSelectedNode = computed(() => nodes.value.some((n) => n.selected))

const items = computed<ListItem[]>(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  const match = (s: string) => !q || s.toLowerCase().includes(q)
  const list: ListItem[] = []
  // 所有项（含控制项）按搜索过滤；节点在 nodes computed 已按名过滤
  if (match('开启代理')) list.push({ type: 'enabled', group: '代理' })
  if (match('规则模式')) list.push({ type: 'mode', group: '代理' })
  list.push(
    ...config.subscriptions
      .filter((s) => match(s.name || s.url || ''))
      .map((s) => ({ type: 'subscription' as const, group: '订阅' as const, sub: s })),
  )
  // 多 selector 分组：显示分组切换项（单分组或无分组时省略）
  if (userGroups.value.length > 1 && match('节点分组')) {
    list.push({ type: 'groupSelector', group: '节点' })
  }
  list.push(...nodes.value.map((n) => ({ type: 'node' as const, group: '节点' as const, node: n })))
  return list
})

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isProxyEnabled)
  } catch (e) {
    console.error('Failed to check proxy status:', e)
  }
}

async function loadCoreStatus() {
  try {
    coreStatus.value = await invoke<{
      downloaded: boolean
      version: string
      downloading: boolean
    }>(CMD.proxyCoreStatus)
  } catch {
    /* ignore */
  }
}

async function downloadCore() {
  if (isDownloading.value) return // 防重入（双击）
  // 乐观标记 + reset 进度，立即反映「下载中」（Rust DOWNLOADING 置位有往返延迟）
  coreStatus.value = { ...coreStatus.value, downloading: true }
  coreProgress.value = { received: 0, total: null }
  progressStarted.value = false
  try {
    await invoke(CMD.proxyEnsureCore)
    // 状态刷新主要由 proxy-core-ready 事件驱动（gunzip 完成即触发），此处兜底
    await loadCoreStatus()
  } catch (e) {
    await loadCoreStatus() // 拉权威状态（失败时 downloading 复位为 false）
    appStore.showStatus(`内核下载失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  }
}

/// 更新内核到最新版：停代理 → 删旧 binary → 重下 → 恢复。复用 progress 事件展示进度。
async function updateCore() {
  if (isDownloading.value) return // 防重入
  coreStatus.value = { ...coreStatus.value, downloading: true }
  coreProgress.value = { received: 0, total: null }
  progressStarted.value = false
  updateInfo.value = null
  try {
    await invoke(CMD.proxyUpdateCore)
    // ready 事件驱动 loadCoreStatus；此处兜底 + 重新查更新
    await loadCoreStatus()
    await checkUpdate()
    appStore.showStatus('内核已更新', { duration: 2000 })
  } catch (e) {
    await loadCoreStatus()
    await checkUpdate()
    appStore.showStatus(`内核更新失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  }
}

/// 检查内核更新：已下载才查（避免未下载时无意义的版本比较）。失败静默（updateInfo 置 null）。
async function checkUpdate() {
  if (!coreStatus.value.downloaded) {
    updateInfo.value = null
    return
  }
  try {
    updateInfo.value = await invoke<{
      hasUpdate: boolean
      current: string
      latest: string
    }>(CMD.proxyCheckUpdate)
  } catch {
    updateInfo.value = null
  }
}

/// 下载按钮文本：下载中 → N% → 解压中。
/// - 未收首字节（建连中 / 退出重进未收到事件）：下载中
/// - 有 Content-Length 且未收齐：N%（百分比分支仅在 received<total 进入，total>0 无除零）
/// - received>=total（含 chunked 完成信号 total=Some(received)）：解压中
/// - chunked（total=null 已收字节）：无法算百分比，诚实回退显示已收字节（避免假造百分比）
const downloadText = computed(() => {
  const { received, total } = coreProgress.value
  if (total != null) {
    if (received >= total) return '解压中'
    return `${Math.floor((received * 100) / total)}%`
  }
  if (progressStarted.value) return `${(received / 1048576).toFixed(1)}MB`
  return '下载中'
})

const toggleEnabled = async () => {
  if (toggling.value) return
  const newState = !isEnabled.value
  if (newState && !config.secret) {
    config.secret = generateRequestId()
  }
  toggling.value = true
  try {
    await invoke(CMD.setProxyEnabled, {
      enabled: newState,
      mixedPort: config.mixedPort,
      controllerPort: config.controllerPort,
      secret: config.secret,
      mode: config.mode,
    })
    // 成功后再翻转状态（toggling 仅作防重入，首次开代理提权时主窗口已隐藏）
    isEnabled.value = newState
    coreError.value = '' // 切换成功清异常提示
    if (newState) {
      await loadProxies()
      prewarmAndTestAll() // 预热后全量测速（fire-and-forget，延迟随测随显）
      startTrafficStream() // 开启实时流量监测
    } else {
      stopTrafficStream()
    }
    // 关闭代理时保留节点列表显示（热重载 idle，不清空 proxiesData）
  } catch (e) {
    appStore.showStatus(`切换失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  } finally {
    toggling.value = false
  }
}

/// 免提权软重启核心（热重载 active config）：代理开着但出站异常时一键重建 gvisor/连接池，
/// 免关闭→开启（规避 stop_root 提权）。进程已退出会失败提示需关闭重开。
async function reconnect() {
  if (toggling.value) return
  toggling.value = true
  try {
    await invoke(CMD.proxyReconnect)
    coreError.value = ''
    appStore.showStatus('代理已重连', { duration: 2000 })
    await loadProxies()
    prewarmAndTestAll()
    startTrafficStream()
  } catch (e) {
    appStore.showStatus(`重连失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  } finally {
    toggling.value = false
  }
}

async function loadProxies() {
  try {
    proxiesData.value = await invoke<ProxiesResponse>(CMD.proxyGetProxies)
    selectedNodeName.value = '' // 已拿到权威 g.now，清空乐观标记
    // 校正分组选择：订阅变更致分组消失或未选时回退首个 user selector
    const names = userGroups.value.map((g) => g.name)
    if (names.length > 0 && !names.includes(activeGroupName.value)) {
      activeGroupName.value = names[0] ?? ''
    }
  } catch (e) {
    // 核心未运行（从未启用 / 进程已退出）时静默；已启用下加载失败才报错
    if (isEnabled.value) {
      appStore.showStatus(`加载节点失败：${errText(e)}`, { duration: 4000, kind: 'error' })
    }
  }
}

async function selectNode(node: NodeItem) {
  if (node.selected) return
  const g = mainGroup.value
  if (!g) return
  try {
    await invoke(CMD.proxySelectProxy, { group: g.name, name: node.name })
    selectedNodeName.value = node.name // 乐观更新，check 立即显示
    await loadProxies()
  } catch (e) {
    appStore.showStatus(`切换节点失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  }
}

async function testAll() {
  if (testing.value || nodes.value.length === 0) return
  testing.value = true
  // 清空已有测速，重新逐个测（随测随显）
  delayMap.value = {}
  try {
    // 限并发（24）+ 逐个回写 delayMap：controller 本地回环扛得住，瓶颈在实际代理连接；
    // 并发太低时差节点吃满超时会占满 worker 致好节点排队干等。随测随显（Promise.all 全量
    // 并发需等最慢节点才批量更新）。
    const CONCURRENCY = 24
    const queue = [...nodes.value]
    const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
      while (queue.length > 0) {
        const n = queue.shift()
        if (!n) break
        try {
          const d = await invoke<number>(CMD.proxyTestDelay, { name: n.name })
          delayMap.value[n.name] = d > 0 ? d : DELAY_TIMEOUT
        } catch {
          delayMap.value[n.name] = DELAY_TIMEOUT
        }
      }
    })
    await Promise.all(workers)
  } finally {
    testing.value = false
  }
}

/// 开启/重连后全量测速前预热：热重载后 mihomo DNS resolver + 节点连接池冷启动，直接 testAll
/// 首轮延迟偏高（远高于 idle 热态）。先测主节点一次预热 generate_204 解析（DNS 缓存全局共享）
/// + 主节点 anytls 连接，紧随的 testAll 命中热态缓存，结果接近真实延迟而非冷启动开销。
async function prewarmAndTestAll() {
  const main = mainGroup.value?.now
  if (main) {
    await invoke<number>(CMD.proxyTestDelay, { name: main }).catch(() => {})
  }
  testAll()
}

/// 定位到当前选中节点：调 BaseList.reveal 平滑滚动到目标项居中
function locateSelected() {
  const idx = items.value.findIndex((it) => it.type === 'node' && it.node.selected)
  if (idx >= 0) baseListRef.value?.reveal(idx)
}

/// 开 /traffic WS 流（开启项副标题实时显示上下行速率）。代理开启时调用。
async function startTrafficStream() {
  if (trafficChannel) return
  const ch = new Channel<TrafficFrame>()
  ch.onmessage = (f) => {
    traffic.value = f
  }
  trafficChannel = ch
  await invoke(CMD.proxyTrafficStream, { onEvent: ch }).catch(() => {
    /* 核心未运行静默 */
  })
}

/// 停 /traffic 流（关代理 / 离开面板）。
function stopTrafficStream() {
  if (!trafficChannel) return
  invoke(CMD.proxyStopStream, { id: 'traffic' }).catch(() => {})
  trafficChannel = null
  traffic.value = null
}

async function onModeChange(value: string | number) {
  const mode = value as typeof config.mode
  config.mode = mode
  if (isEnabled.value) {
    try {
      await invoke(CMD.proxySetMode, { mode })
    } catch (e) {
      appStore.showStatus(`切换模式失败：${errText(e)}`, { duration: 4000, kind: 'error' })
    }
  }
}

/// 回车/双击分派
function onExecute(item: unknown) {
  if (appStore.isComposing) return
  const it = item as ListItem | undefined
  if (!it) return
  if (it.type === 'enabled') {
    if (coreStatus.value.downloaded) toggleEnabled()
    else downloadCore()
  } else if (it.type === 'node') selectNode(it.node)
  else if (it.type === 'subscription') openEditModal(it.sub)
}

// ── 订阅 ──
function formatTime(ts: string): string {
  if (!ts) return '未更新'
  return ts.slice(0, 10)
}

/// 组标题「+」：打开新建弹窗（不预创建项，保存时才 add）
function openCreateModal() {
  editingId.value = ''
  isCreating.value = true
  editForm.value = { name: '', url: '' }
  showEditModal.value = true
}

function openEditModal(s: Subscription) {
  editingId.value = s.id
  isCreating.value = false
  editForm.value = { name: s.name, url: s.url }
  showEditModal.value = true
}

function closeEditModal() {
  showEditModal.value = false
  editingId.value = ''
  isCreating.value = false
}

/// 从订阅链接提取域名主体作为默认名称（如 https://a.example.com → example）
function domainFromUrl(url: string): string {
  try {
    const parts = new URL(url).hostname.split('.')
    return parts.length >= 2 ? parts[parts.length - 2] : (parts[0] ?? '')
  } catch {
    return ''
  }
}

/// url 变化时，若用户未手填 name 则自动截取域名
watch(
  () => editForm.value.url,
  (url) => {
    if (!editForm.value.name.trim()) {
      editForm.value.name = domainFromUrl(url)
    }
  },
)

/// 保存：新建则 add，编辑则 update；url 非空则拉取（含热重启 + 节点刷新）
async function saveSub() {
  const name = editForm.value.name.trim()
  const url = editForm.value.url.trim()
  let id: string
  if (isCreating.value) {
    id = addSubscription(name, url)
  } else {
    id = editingId.value
    if (!id) return
    updateSubscription(id, { name, url })
  }
  closeEditModal()
  if (!url) return
  try {
    const count = await invoke<number>(CMD.proxyUpdateSubscription, { id, url })
    updateSubscription(id, { proxyCount: count, updatedAt: new Date().toISOString() })
    appStore.showStatus(`已更新 ${count} 个节点`, { duration: 2000 })
    // 不自动开启代理（尊重用户显式关闭）；已开启时刷新节点列表应用新订阅
    if (isEnabled.value) await loadProxies()
  } catch (e) {
    appStore.showStatus(`更新失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  }
}

/// 编辑弹窗内删除 → 二次确认
function confirmRemoveFromModal() {
  const s = config.subscriptions.find((x) => x.id === editingId.value)
  if (!s) return
  closeEditModal()
  deletingSub.value = s
}

async function doRemoveSub() {
  const s = deletingSub.value
  if (!s) return
  deletingSub.value = null
  removeSubscription(s.id)
  try {
    await invoke(CMD.proxyRemoveSubscription, { id: s.id })
    // 热重启完成（含 wait_ready）后刷新，节点列表移除该订阅节点
    if (isEnabled.value) await loadProxies()
  } catch (e) {
    appStore.showStatus(`清理订阅失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  }
}

onMounted(async () => {
  unlistenProgress = await listen<{ received: number; total: number | null }>(
    'proxy-core-progress',
    (e) => {
      coreProgress.value = e.payload
      progressStarted.value = true
    },
  )
  // gunzip 完成（bin 可用）后事件驱动刷新，绕过 invoke(proxyEnsureCore) resolve 时序
  unlistenReady = await listen('proxy-core-ready', () => {
    loadCoreStatus()
    checkUpdate() // 更新/下载完成后重新查更新（更新后 hasUpdate 应为 false）
  })
  unlistenEnabled = await listen<boolean>('proxy-enabled', (e) => {
    // 切换中由 toggleEnabled 成功后统一设值，忽略命令内提前 emit 的事件防回声
    if (toggling.value) return
    isEnabled.value = e.payload
    if (!e.payload) stopTrafficStream() // 关代理（含菜单关闭/进程退出）停流量流
  })
  unlistenMode = await listen<string>('proxy-mode', (e) => {
    config.mode = e.payload as typeof config.mode
  })
  // 健康监测异常反馈：进程异常退出/出站失效自动恢复失败时，核心 emit proxy-status。
  // error 持久写入 coreError（开启代理项红色提示，enabled 态附重连按钮）+ 状态栏即时提醒。
  unlistenStatus = await listen<{ kind: string; msg: string }>('proxy-status', (e) => {
    const { kind, msg } = e.payload
    coreError.value = kind === 'error' ? msg : ''
    if (msg) {
      appStore.showStatus(msg, {
        duration: 4000,
        kind: kind === 'error' ? 'error' : 'success',
      })
    }
  })
  await loadCoreStatus()
  await checkStatus()
  // 核心已下载即加载节点列表（idle 常驻下 controller 仍可查询；
  // 核心未运行时 loadProxies 静默失败，不报错）
  if (coreStatus.value.downloaded) {
    await loadProxies()
  }
  // 已启用代理时开启实时流量监测（首渲染时 onActivated 先于 onMounted async 完成，
  // isEnabled 尚未就绪，故此处补开；后续切回面板由 onActivated 驱动）
  if (isEnabled.value) await startTrafficStream()
})

// 面板激活时（含首次挂载后）查更新：已下载才查，API 不可达静默降级。
// 重激活也触发，让用户切回面板即可看到最新版本提示（API rate limit 60/h 够自用）。
// 同时恢复流量流（切子视图时 onDeactivated 停止，切回时重启；startTrafficStream 有防重入守卫）。
onActivated(() => {
  checkUpdate()
  if (isEnabled.value) startTrafficStream()
})

// 切子视图（连接/规则/日志）时被 KeepAlive 缓存：停流量流免空转。
onDeactivated(() => {
  stopTrafficStream()
})

onUnmounted(() => {
  unlistenProgress?.()
  unlistenReady?.()
  unlistenEnabled?.()
  unlistenMode?.()
  unlistenStatus?.()
  stopTrafficStream()
})

// 兜底：订阅列表至少一项（磁盘旧值可能为空数组，覆盖默认项；类似 agent 默认 provider 始终存在）
watch(
  () => config.subscriptions.length,
  (len) => {
    if (len === 0) addSubscription('', '')
  },
  { immediate: true },
)
</script>
