<template>
  <div h="full" overflow="y-auto">
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
          :subtitle="enabledSubtitle"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              v-if="coreStatus.downloaded"
              :disabled="toggling"
              :variant="isEnabled ? 'primary' : 'default'"
              @click.stop="toggleEnabled"
            >
              {{ toggling ? '处理中' : isEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
            <BaseButton v-else :disabled="downloadingCore" @click.stop="downloadCore">
              {{ downloadingCore ? (coreProgress >= 100 ? '解压中' : `${coreProgress}%`) : '下载' }}
            </BaseButton>
          </template>
        </BaseListItem>

        <!-- TUN 模式 -->
        <BaseListItem
          v-else-if="item.type === 'tun'"
          :ref="setRef"
          title="TUN 模式"
          subtitle="全局透明代理，接管所有流量"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton :variant="config.tunMode ? 'primary' : 'default'" @click.stop="toggleTun">
              {{ config.tunMode ? '已开启' : '已关闭' }}
            </BaseButton>
          </template>
        </BaseListItem>

        <!-- 规则模式 -->
        <BaseListItem
          v-else-if="item.type === 'mode'"
          :ref="setRef"
          title="规则模式"
          subtitle="规则按分流策略，全局代理所有流量，直连不经过代理"
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

        <!-- 节点加载占位 -->
        <BaseListItem
          v-else-if="item.type === 'loading'"
          :ref="setRef"
          title="节点加载中…"
          :selected="selected"
        >
          <template #trailing>
            <span class="i-ri-loader-4-line text-base text-tx-muted animate-spin" />
          </template>
        </BaseListItem>

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
            <span :class="delayColor(item.node.delay)" class="text-xs font-medium text-right w-10">
              {{ formatDelay(item.node.delay) }}
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
          <BaseInput v-model="editForm.name" placeholder="订阅名称" />
        </div>
        <div class="form-field">
          <span class="form-label">订阅链接</span>
          <BaseInput v-model="editForm.url" placeholder="订阅链接" />
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
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
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
  delayColor,
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

type ListItem =
  | { type: 'enabled'; group: '代理' }
  | { type: 'tun'; group: '代理' }
  | { type: 'mode'; group: '代理' }
  | { type: 'subscription'; group: '订阅'; sub: Subscription }
  | { type: 'groupSelector'; group: '节点' }
  | { type: 'node'; group: '节点'; node: NodeItem }
  | { type: 'loading'; group: '节点' }

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
const coreProgress = ref(0)
const downloadingCore = ref(false)
let unlistenProgress: (() => void) | null = null
// 托盘菜单切换 TUN 时，Rust 广播 proxy-tun → 同步 config.tunMode（Rust 不回写 plugin-store，
// 否则下次开面板 TUN 按钮显示旧值且会发出错误翻转）。
let unlistenTun: (() => void) | null = null
// 菜单栏改开关/规则模式/切节点时，Rust 广播 → 同步面板（命令内含「未变跳过」守卫防 watch 回声）
let unlistenEnabled: (() => void) | null = null
let unlistenMode: (() => void) | null = null
// 菜单栏切换节点后广播 → 重新拉取节点列表同步选中项
let unlistenNode: (() => void) | null = null

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

// 当前展示的分组：用户选中 > 首个 user selector > GLOBAL
const mainGroup = computed(() => {
  if (!proxiesData.value) return null
  const proxies = proxiesData.value.proxies
  const groups = userGroups.value
  if (groups.length === 0) return proxies['GLOBAL'] ?? null
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
  return g.all.map((name) => {
    const entry = proxiesData.value?.proxies[name]
    return {
      id: name,
      name,
      delay: delayMap.value[name] ?? latestDelay(entry?.history),
      selected: current === name,
    }
  })
})

/// 是否存在选中节点（定位按钮 disabled 判断）
const hasSelectedNode = computed(() => nodes.value.some((n) => n.selected))

const items = computed<ListItem[]>(() => {
  const list: ListItem[] = [
    { type: 'enabled', group: '代理' },
    { type: 'tun', group: '代理' },
    { type: 'mode', group: '代理' },
  ]
  list.push(
    ...config.subscriptions.map((s) => ({
      type: 'subscription' as const,
      group: '订阅' as const,
      sub: s,
    })),
  )
  // 多 selector 分组：显示分组切换项（单分组或无分组时省略）
  if (userGroups.value.length > 1) {
    list.push({ type: 'groupSelector', group: '节点' })
  }
  // 启动中且暂无节点：显示加载占位，避免节点区空白滞后
  if (toggling.value && nodes.value.length === 0) {
    list.push({ type: 'loading', group: '节点' })
  } else {
    list.push(
      ...nodes.value.map((n) => ({ type: 'node' as const, group: '节点' as const, node: n })),
    )
  }
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
  coreProgress.value = 0
  downloadingCore.value = true
  try {
    await invoke(CMD.proxyEnsureCore)
    downloadingCore.value = false
    await loadCoreStatus()
    // 内核就绪：自动启用代理核心以显示节点
    if (coreStatus.value.downloaded && !isEnabled.value) {
      await toggleEnabled()
    }
  } catch (e) {
    downloadingCore.value = false
    appStore.showStatus(`内核下载失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  }
}

/// 启用代理项 subtitle：有内核显示版本，无内核提示下载
const enabledSubtitle = computed(() => {
  if (coreStatus.value.downloaded) {
    return `内核版本：mihomo ${coreStatus.value.version}`
  }
  if (downloadingCore.value) return '正在下载内核…'
  return '需先下载内核'
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
      tun: config.tunMode,
    })
    // 成功后再翻转状态：切换期间按钮显示「处理中」并保持原样式，完成后才变（避免样式先于反馈）
    isEnabled.value = newState
    if (newState) {
      // 系统代理由 Rust set_proxy_enabled 按 user/TUN 模式自动应用，前端无需干预
      await loadProxies()
    }
    // 关闭代理时保留节点列表显示（仅停止 mihomo，不清空 proxiesData）
  } catch (e) {
    appStore.showStatus(`切换失败：${errText(e)}`, { duration: 4000, kind: 'error' })
  } finally {
    toggling.value = false
  }
}

async function loadProxies() {
  if (!isEnabled.value) return // 未启用时保留上次节点列表，不清空
  try {
    proxiesData.value = await invoke<ProxiesResponse>(CMD.proxyGetProxies)
    selectedNodeName.value = '' // 已拿到权威 g.now，清空乐观标记
    // 校正分组选择：订阅变更致分组消失或未选时回退首个 user selector
    const names = userGroups.value.map((g) => g.name)
    if (names.length > 0 && !names.includes(activeGroupName.value)) {
      activeGroupName.value = names[0] ?? ''
    }
  } catch (e) {
    appStore.showStatus(`加载节点失败：${errText(e)}`, { duration: 4000, kind: 'error' })
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
  try {
    // 限并发（8）+ 逐个回写 delayMap：既避免百节点订阅打满 controller，
    // 又让延迟随测随显（Promise.all 全量并发需等最慢节点才批量更新）。
    const CONCURRENCY = 8
    const queue = [...nodes.value]
    const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
      while (queue.length > 0) {
        const n = queue.shift()
        if (!n) break
        try {
          delayMap.value[n.name] = await invoke<number>(CMD.proxyTestDelay, { name: n.name })
        } catch {
          delayMap.value[n.name] = 0
        }
      }
    })
    await Promise.all(workers)
  } finally {
    testing.value = false
  }
}

/// 定位到当前选中节点：调 BaseList.reveal 平滑滚动到目标项居中
function locateSelected() {
  const idx = items.value.findIndex((it) => it.type === 'node' && it.node.selected)
  if (idx >= 0) baseListRef.value?.reveal(idx)
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
  if (it.type === 'enabled') toggleEnabled()
  else if (it.type === 'tun') toggleTun()
  else if (it.type === 'node') selectNode(it.node)
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
    if (!isEnabled.value) {
      await toggleEnabled()
    } else {
      await loadProxies()
    }
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

// ── 开关 ──
async function toggleTun() {
  const newVal = !config.tunMode
  config.tunMode = newVal
  if (isEnabled.value) {
    try {
      await invoke(CMD.proxyEnableTun, { tun: newVal })
    } catch (e) {
      const msg = errText(e)
      if (!msg.includes('未运行')) {
        config.tunMode = !newVal
        appStore.showStatus(`TUN 切换失败：${msg}`, { duration: 4000, kind: 'error' })
      }
    }
  }
}

onMounted(async () => {
  unlistenProgress = await listen<number>('proxy-core-progress', (e) => {
    coreProgress.value = e.payload
  })
  unlistenTun = await listen<boolean>('proxy-tun', (e) => {
    config.tunMode = e.payload
  })
  unlistenEnabled = await listen<boolean>('proxy-enabled', (e) => {
    // 切换中由 toggleEnabled 成功后统一设值，忽略命令内提前 emit 的事件以防样式先于「处理中」反馈
    if (toggling.value) return
    isEnabled.value = e.payload
  })
  unlistenMode = await listen<string>('proxy-mode', (e) => {
    config.mode = e.payload as typeof config.mode
  })
  unlistenNode = await listen<string>('proxy-node', () => {
    // 菜单栏切节点后重拉节点列表（loadProxies 内有 isEnabled 守卫）
    if (isEnabled.value) loadProxies()
  })
  await loadCoreStatus()
  await checkStatus()
  // 仅按已持久化的 enabled 状态恢复节点列表；不自动启用核心——尊重用户上一次显式关闭
  // （下载内核 / 添加订阅这两个主动操作仍会在各自流程内自启）。
  if (isEnabled.value) {
    await loadProxies()
  }
})

onUnmounted(() => {
  unlistenProgress?.()
  unlistenTun?.()
  unlistenEnabled?.()
  unlistenMode?.()
  unlistenNode?.()
})

// 订阅名同步到菜单栏（Rust 缓存 menu_subs 供订阅子菜单展示；订阅名变更即推送）
watch(
  () => config.subscriptions.map((s) => s.name).join('\n'),
  () => {
    invoke(CMD.proxySyncMenuSubs, {
      names: config.subscriptions.map((s) => s.name),
    }).catch((e: unknown) => console.error('[proxy] sync menu subs failed:', e))
  },
  { immediate: true },
)

// 兜底：订阅列表至少一项（磁盘旧值可能为空数组，覆盖默认项；类似 agent 默认 provider 始终存在）
watch(
  () => config.subscriptions.length,
  (len) => {
    if (len === 0) addSubscription('', '')
  },
  { immediate: true },
)
</script>
