/// proxy 主视图状态与动作（View.vue 仅模板）。订阅数据/自动更新见 ./subscriptions，
/// 订阅列表交互见订阅二级界面 ./useSubscriptions（本模块只保留入口行与节点列表联动）。

import { ref, computed, onMounted, onActivated, onDeactivated, onUnmounted, watch } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isTauri } from '@/utils/tauri'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { config } from './config'
import { activeSubscription } from './subscriptions'
import { generateRequestId } from '@/utils/id'
import {
  type ProxiesResponse,
  type DelayResult,
  DELAY_TIMEOUT,
  delayColor,
  filterNodes,
  formatDelay,
  formatSubExpiry,
  isUserSelectorGroup,
  latestDelay,
} from './logic'
import { formatRate, toErrorMessage } from '@/utils/format'
import type BaseSelect from '@/components/ui/BaseSelect.vue'

export interface NodeItem {
  id: string
  name: string
  delay: number
  selected: boolean
}

/** mihomo /traffic WS 帧（上下行速率 bytes/s）。 */
export interface TrafficFrame {
  up: number
  down: number
}

export type ListItem =
  | { type: 'enabled'; group: string }
  | { type: 'mode'; group: string }
  | { type: 'subscriptions'; group: string }
  | { type: 'groupSelector'; group: string }
  | { type: 'node'; group: string; node: NodeItem }

/// 预加载代理运行状态：本模块随 index.ts eager 加载（app 启动早期）即触发 IPC 往返，
/// useProxyPanel 创建时同步读缓存作 ref 初始值——首帧即真实值，消除默认 false→true 的渲染闪烁。
/// app 启动到用户打开 proxy 视图的间隔远大于 IPC 往返，缓存几乎一定已就绪。
/// preloadPromise 保存引用供 onMounted 复用（已完成即时 resolve，未完成 await 同一 in-flight，
/// 不重新发 IPC）——避免 onMounted 重复 IPC 往返导致按钮延迟出现。
const preloaded = {
  done: false,
  enabled: false,
  coreDownloaded: false,
  coreDownloading: false,
  version: '',
}
let preloadPromise: Promise<void> | null = null
if (isTauri) {
  preloadPromise = Promise.all([
    invoke<boolean>(CMD.isProxyEnabled)
      .then((v) => {
        preloaded.enabled = v
      })
      .catch(() => {}),
    invoke<{
      downloaded: boolean
      version: string
      downloading: boolean
    }>(CMD.proxyCoreStatus)
      .then((s) => {
        preloaded.coreDownloaded = s.downloaded
        preloaded.coreDownloading = s.downloading
        preloaded.version = s.version
      })
      .catch(() => {}),
  ])
    .then(() => {
      preloaded.done = true
    })
    .catch(() => {
      preloaded.done = true
    })
}

export function useProxyPanel() {
  const appStore = useAppStore()
  /** proxy 流量速率紧凑口径（1.2K/s） */
  const fmtTrafficRate = (n: number) => formatRate(n, { compact: true })
  const isEnabled = ref(preloaded.enabled)
  /// 状态就绪：预加载 done 时首帧 true（按钮直接渲染正确值）；否则 onMounted 复用 preloadPromise
  /// 完成后翻 true。false 时 View 开启代理项不渲染 trailing/subtitle（避免错误态闪烁）。
  const statusLoaded = ref(preloaded.done)
  const toggling = ref(false)
  const proxiesData = ref<ProxiesResponse | null>(null)
  const delayMap = ref<Record<string, number>>({})
  const testing = ref(false)
  const selectedIndex = ref(0)
  const baseListRef = ref<{ reveal: (i: number) => void } | null>(null)
  const modeSelectRef = ref<InstanceType<typeof BaseSelect> | null>(null)
  const groupSelectRef = ref<InstanceType<typeof BaseSelect> | null>(null)
  const coreStatus = ref<{
    downloaded: boolean
    version: string
    downloading: boolean
  }>({
    downloaded: preloaded.coreDownloaded,
    version: preloaded.version,
    downloading: preloaded.coreDownloading,
  })
  const coreProgress = ref<{ received: number; total: number | null }>({ received: 0, total: null })
  /// 首个进度事件是否到达：未收到事件时显示「下载中」，收到后显示具体进度
  const progressStarted = ref(false)
  /// 核心更新检查结果（hasUpdate=true 时副标题提示 + 显示「更新」按钮）。null=未检查/API 失败
  const updateInfo = ref<{ hasUpdate: boolean; current: string; latest: string } | null>(null)
  /// 健康监测异常状态（proxy-status error 事件）：非空时开启代理项显示红色提示 + 重连按钮。
  const coreError = ref('')
  /// 实时流量速率（开代理时经 /traffic WS 推送，视图顶部流量面板展示）。
  const traffic = ref<TrafficFrame | null>(null)
  /// 流量曲线窗口（60s，/traffic 每秒一帧，与 TrafficSparkline 的窗口容量对齐）；
  /// 停止流即清空，重新开启从零起笔。
  const TRAFFIC_HISTORY_SIZE = 60
  const trafficHistory = ref<TrafficFrame[]>([])
  const trafficDownHistory = computed(() => trafficHistory.value.map((f) => f.down))
  const trafficUpHistory = computed(() => trafficHistory.value.map((f) => f.up))
  /// 视图当前是否激活（KeepAlive activate/deactivate 驱动）：外部置位 enabled 时
  /// 仅激活中才开流量流，非激活留给 onActivated 补开（避免后台空转 WS）。
  const viewActive = ref(false)
  let trafficChannel: Channel<TrafficFrame> | null = null
  /// 下载中状态真相源跟随 Rust DOWNLOADING 原子（重新进入界面也能正确反映）。
  const isDownloading = computed(() => coreStatus.value.downloading)
  let unlistenProgress: (() => void) | null = null
  // Rust gunzip 完成（bin 可用）后 emit，事件驱动刷新状态，不依赖 invoke resolve 时序
  let unlistenReady: (() => void) | null = null
  let unlistenEnabled: (() => void) | null = null
  let unlistenMode: (() => void) | null = null
  let unlistenStatus: (() => void) | null = null
  let unlistenSubUpdated: (() => void) | null = null

  // ── 订阅入口 ──
  /// 主视图只保留当前激活订阅入口（完整列表在二级界面）：标题=名称，
  /// 副标题=节点数 + 到期日（无节点回退订阅链接）。行内红色失败标记在模板单列。
  const activeSubscriptionTitle = computed(
    () => activeSubscription.value?.name || t('proxy.unnamedSubscription'),
  )
  const activeSubscriptionSubtitle = computed(() => {
    const s = activeSubscription.value
    if (!s) return ''
    if (!s.proxyCount) return s.url || t('proxy.notConfigured')
    const parts = [t('proxy.subscriptionCount', { count: s.proxyCount })]
    const expiry = formatSubExpiry(s.expiresAt)
    if (expiry) parts.push(t('proxy.subscriptionExpires', { date: expiry }))
    return parts.join(' · ')
  })
  /// 节点列表待重载：二级界面切换激活订阅 / 激活订阅内容更新时置脏，本视图停用期间
  /// 不后台拉取，回到主视图的 onActivated 统一重载（避免隐藏视图渲染与请求抖动）。
  let nodesDirty = false

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

  const groupOptions = computed(() =>
    userGroups.value.map((g) => ({ label: g.name, value: g.name })),
  )

  /// 规则模式选项（localized label，响应 locale 切换）
  const MODE_OPTIONS = computed(() => [
    { label: t('proxy.mode.rule'), value: 'rule' as const },
    { label: t('proxy.mode.global'), value: 'global' as const },
  ])

  // 当前展示的分组：用户选中 > 首个 user selector。无 selector（无订阅）返回 null——
  // 不回退 GLOBAL（其 all 仅含 DIRECT/REJECT 内置策略，非真实代理节点，展示无意义）
  const mainGroup = computed(() => {
    if (!proxiesData.value) return null
    const groups = userGroups.value
    if (groups.length === 0) return null
    const chosen = activeGroupName.value
      ? groups.find((g) => g.name === activeGroupName.value)
      : null
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
    if (match(t('proxy.enableProxy'))) list.push({ type: 'enabled', group: t('proxy.group.proxy') })
    if (match(t('proxy.ruleMode'))) list.push({ type: 'mode', group: t('proxy.group.proxy') })
    // 订阅收束为单个入口行（列表在二级界面）：名称 / 链接 / 「订阅」均可命中
    if (
      match(activeSubscriptionTitle.value) ||
      match(activeSubscription.value?.url ?? '') ||
      match(t('proxy.group.subscription'))
    ) {
      list.push({ type: 'subscriptions', group: t('proxy.group.subscription') })
    }
    // 多 selector 分组：显示分组切换项（单分组或无分组时省略）
    if (userGroups.value.length > 1 && match(t('proxy.nodeGroup'))) {
      list.push({ type: 'groupSelector', group: t('proxy.group.nodes') })
    }
    list.push(
      ...nodes.value.map((n) => ({
        type: 'node' as const,
        group: t('proxy.group.nodes'),
        node: n,
      })),
    )
    return list
  })

  async function loadCoreStatus() {
    try {
      coreStatus.value = await invoke<{
        downloaded: boolean
        version: string
        downloading: boolean
      }>(CMD.proxyCoreStatus)
      preloaded.coreDownloaded = coreStatus.value.downloaded
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
      appStore.showStatus(toErrorMessage(e, t('proxy.coreDownloadFailed')), {
        duration: 4000,
        kind: 'error',
      })
    }
  }

  /// 更新核心到最新版：停代理 → 删旧 binary → 重下 → 恢复。复用 progress 事件展示进度。
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
      appStore.showStatus(t('proxy.coreUpdated'), { duration: 2000 })
    } catch (e) {
      await loadCoreStatus()
      await checkUpdate()
      appStore.showStatus(toErrorMessage(e, t('proxy.coreUpdateFailed')), {
        duration: 4000,
        kind: 'error',
      })
    }
  }

  /// 检查核心更新：已下载才查（避免未下载时无意义的版本比较）。失败静默（updateInfo 置 null）。
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
      if (received >= total) return t('proxy.extracting')
      return `${Math.floor((received * 100) / total)}%`
    }
    if (progressStarted.value) return `${(received / 1048576).toFixed(1)}MB`
    return t('proxy.downloading')
  })

  const toggleEnabled = async () => {
    if (toggling.value) return
    const newState = !isEnabled.value
    // 首次启用确认：TUN 是全部扩展中最重的系统侵入面（LaunchDaemon + root 常驻进程 +
    // 接管全部 IP 流量），安装前明确告知。以持久化确认标记为据只弹一次——daemon 安装态
    // 不可作判据（更新核心/完全卸载/提权取消都会移除 daemon，已确认用户会被重复打扰）。
    if (newState && !config.tunConfirmed) {
      const confirmed = await appStore.showConfirm({
        title: t('proxy.tunConfirmTitle'),
        message: t('proxy.tunConfirmMessage'),
        // lg：首条要点的 /Library/LaunchDaemons/…mihomo.plist 行内代码约 280px，
        // md（~400px 内容宽）下与中文前缀合计溢出致路径中间断行
        size: 'lg',
        markdown: true,
        okLabel: t('proxy.tunConfirmOk'),
      })
      if (!confirmed) return
      config.tunConfirmed = true // 确认即置位：同意安装后重试（提权取消/安装失败）不再弹
    }
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
        activeSubId: config.activeSubscriptionId,
      })
      // 成功后再翻转状态（toggling 仅作防重入，首次开代理提权时主窗口已隐藏）
      isEnabled.value = newState
      preloaded.enabled = newState
      coreError.value = '' // 切换成功清异常提示
      if (newState) {
        await loadProxies()
        testAll() // 全量测速（fire-and-forget，批量端点 mihomo 内部并发）
        startTrafficStream() // 开启实时流量监测
      } else {
        stopTrafficStream()
      }
      // 关闭代理时保留节点列表显示（热重载 idle，不清空 proxiesData）
    } catch (e) {
      appStore.showStatus(toErrorMessage(e, t('proxy.switchFailed')), {
        duration: 4000,
        kind: 'error',
      })
    } finally {
      toggling.value = false
    }
  }

  /// 免提权软重启核心（热重载 active config）：代理开着但出站异常时一键重建 TUN 栈/连接池，
  /// 免关闭→开启（launchd 托管下关闭重开本就免提权，但软重启更快不中断）。进程已退出会失败提示需关闭重开。
  async function reconnect() {
    if (toggling.value) return
    toggling.value = true
    try {
      await invoke(CMD.proxyReconnect)
      coreError.value = ''
      appStore.showStatus(t('proxy.reconnected'), { duration: 2000 })
      await loadProxies()
      testAll()
      startTrafficStream()
    } catch (e) {
      appStore.showStatus(toErrorMessage(e, t('proxy.reconnectFailed')), {
        duration: 4000,
        kind: 'error',
      })
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
        appStore.showStatus(toErrorMessage(e, t('proxy.loadNodesFailed')), {
          duration: 4000,
          kind: 'error',
        })
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
      appStore.showStatus(toErrorMessage(e, t('proxy.switchNodeFailed')), {
        duration: 4000,
        kind: 'error',
      })
    }
  }

  async function testAll() {
    if (testing.value) return
    const g = mainGroup.value
    if (!g) return
    testing.value = true
    // 流式增量：每节点测完即经 Channel 推送，逐项写入 delayMap 触发该节点重渲染。
    // 不预清空——测速中未到的节点保留上次值/历史值，避免整体闪烁；结果逐个覆盖。
    // 快照组名防串组：测速进行中切换 mainGroup，旧组在飞的回调不再写入 delayMap，避免过期条目残留。
    const snapshotGroup = g.name
    const ch = new Channel<DelayResult>()
    ch.onmessage = (r) => {
      if (mainGroup.value?.name !== snapshotGroup) return
      delayMap.value = { ...delayMap.value, [r.name]: r.delay > 0 ? r.delay : DELAY_TIMEOUT }
    }
    try {
      await invoke(CMD.proxyTestGroupDelayStream, { group: g.name, onEvent: ch })
    } catch {
      // 整体失败（controller 不可达 / 节点列表获取失败）：仍在该组时标记全组超时保持反馈
      if (mainGroup.value?.name !== snapshotGroup) return
      const map = { ...delayMap.value }
      for (const name of g.all ?? []) map[name] = DELAY_TIMEOUT
      delayMap.value = map
    } finally {
      testing.value = false
    }
  }

  /// 定位到当前选中节点：调 BaseList.reveal 平滑滚动到目标项居中
  function locateSelected() {
    const idx = items.value.findIndex((it) => it.type === 'node' && it.node.selected)
    if (idx >= 0) baseListRef.value?.reveal(idx)
  }

  /// 开 /traffic WS 流（视图顶部流量面板实时显示上下行速率与曲线）。代理开启时调用。
  async function startTrafficStream() {
    if (trafficChannel) return
    const ch = new Channel<TrafficFrame>()
    ch.onmessage = (f) => {
      traffic.value = f
      // 环形窗口：逐秒追加，满 60s 丢弃最老样本（曲线右端恒为最新）
      trafficHistory.value = [...trafficHistory.value.slice(-(TRAFFIC_HISTORY_SIZE - 1)), f]
    }
    trafficChannel = ch
    await invoke(CMD.proxyTrafficStream, { onEvent: ch }).catch(() => {
      /* 核心未运行静默 */
    })
  }

  /// 停 /traffic 流（关代理 / 离开视图）。
  function stopTrafficStream() {
    if (!trafficChannel) return
    invoke(CMD.proxyStopStream, { id: 'traffic' }).catch(() => {})
    trafficChannel = null
    traffic.value = null
    trafficHistory.value = []
  }

  async function onModeChange(value: string | number) {
    const mode = value as typeof config.mode
    const prev = config.mode
    config.mode = mode
    if (isEnabled.value) {
      try {
        await invoke(CMD.proxySetMode, { mode })
      } catch (e) {
        config.mode = prev // 失败回滚：持久化偏好与运行态保持一致（否则重启后才生效）
        appStore.showStatus(toErrorMessage(e, t('proxy.switchModeFailed')), {
          duration: 4000,
          kind: 'error',
        })
      }
    }
  }

  /// 回车/双击分派
  function onExecute(item: unknown) {
    if (appStore.isComposing) return
    const it = item as ListItem | undefined
    if (!it) return
    if (it.type === 'enabled') {
      // 默认动作恒为开关连接：有核心新版时更新按钮只是次级入口，回车不改变主行为
      if (coreStatus.value.downloaded) toggleEnabled()
      else downloadCore()
    } else if (it.type === 'mode') {
      modeSelectRef.value?.focus()
      modeSelectRef.value?.toggleOpen()
    } else if (it.type === 'groupSelector') {
      groupSelectRef.value?.focus()
      groupSelectRef.value?.toggleOpen()
    } else if (it.type === 'node') selectNode(it.node)
    else if (it.type === 'subscriptions') openSubscriptions()
  }

  /// 进入订阅二级界面（列表 / 添加 / 编辑 / 更新 / 激活切换）：清空主视图过滤词——
  /// 节点搜索词不应带进订阅列表（两屏的 query 语义不同）。
  function openSubscriptions() {
    appStore.setSearchQuery('')
    appStore.openSubview('subscriptions')
  }

  /// 订阅变化致节点列表失效（切换激活订阅 / 激活订阅内容更新 / 删除激活订阅）：
  /// 清乐观选中与测速缓存。本视图停用（二级界面正在操作）时只置脏，回到本视图的
  /// onActivated 统一重载——隐藏期间不后台拉取，避免停用视图渲染与首帧旧数据闪烁。
  function invalidateNodes() {
    selectedNodeName.value = ''
    delayMap.value = {}
    nodesDirty = true
  }

  // 激活订阅切换（二级界面写入 config）：节点列表整体替换，语义同失效。
  watch(
    () => config.activeSubscriptionId,
    () => {
      invalidateNodes()
      if (viewActive.value) {
        nodesDirty = false
        void loadProxies()
      }
    },
  )

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
      preloaded.enabled = e.payload
      if (e.payload) {
        // 外部置位开启（菜单栏切换 / 启动重连恢复）：刷新节点列表（onActivated 不重载，
        // 不刷会停留陈旧空列表）；流量流仅激活中开，非激活由 onActivated 补开
        void loadProxies()
        if (viewActive.value) startTrafficStream()
      } else {
        stopTrafficStream() // 关代理（含菜单关闭/进程退出）停流量流
      }
    })
    unlistenMode = await listen<string>('proxy-mode', (e) => {
      config.mode = e.payload as typeof config.mode
    })
    // 激活订阅内容被更新（二级界面行内更新 / 自动更新 / 编辑保存）：节点列表整体替换。
    // 二级界面操作期间本视图停用，仅置脏；回到本视图的 onActivated 统一重载。
    unlistenSubUpdated = await listen<string>('proxy-subscription-updated', (e) => {
      if (e.payload !== config.activeSubscriptionId) return
      invalidateNodes()
      if (viewActive.value) {
        nodesDirty = false
        void loadProxies()
      }
    })
    // 健康监测异常反馈：进程异常退出/出站失效自动恢复失败时，核心 emit proxy-status。
    // error 持久写入 coreError（开启代理项红色提示，enabled 态附重连按钮）；
    // toast 通道由扩展入口（proxy/index.ts）模块级常驻监听承载——面板未打开过
    //（KeepAlive 不存活）时菜单栏路径的失败提示不因此丢失。
    unlistenStatus = await listen<{ kind: string; msg: string }>('proxy-status', (e) => {
      coreError.value = e.payload.kind === 'error' ? e.payload.msg : ''
    })
    // 复用预加载 Promise：已完成则即时（preloaded.done=true，statusLoaded 首帧已 true）；
    // 未完成则 await 同一个 in-flight Promise（不重新发 IPC），完成后从缓存同步 ref。
    // 避免重复 IPC 往返——那会延迟 statusLoaded 翻 true，
    // 用户从菜单栏打开窗口时感知为"按钮延迟出现"。
    if (preloadPromise) await preloadPromise
    isEnabled.value = preloaded.enabled
    coreStatus.value = {
      downloaded: preloaded.coreDownloaded,
      version: preloaded.version,
      downloading: preloaded.coreDownloading,
    }
    statusLoaded.value = true
    // 重连恢复竞态兜底：Rust setup 的 reconnect_root_mihomo 异步跑，可能晚于模块预加载
    // 把 enabled 恢复为 true（启动时代理仍在生效）。挂载时再查一次权威值——单条原子读
    // IPC，statusLoaded 已置位不阻塞首帧渲染。
    try {
      const fresh = await invoke<boolean>(CMD.isProxyEnabled)
      isEnabled.value = fresh
      preloaded.enabled = fresh
    } catch {
      /* ignore */
    }
    // 核心已下载即加载节点列表（idle 常驻下 controller 仍可查询；
    // 核心未运行时 loadProxies 静默失败，不报错）
    if (coreStatus.value.downloaded) {
      await loadProxies()
    }
    // 已启用代理时开启实时流量监测（首渲染时 onActivated 先于 onMounted async 完成，
    // isEnabled 尚未就绪，故此处补开；后续切回视图由 onActivated 驱动）
    if (isEnabled.value) await startTrafficStream()
  })

  // 视图激活时（含首次挂载后）查更新：已下载才查，API 不可达静默降级。
  // 重激活也触发，让用户切回视图即可看到最新版本提示（API rate limit 60/h 够自用）。
  // 同时恢复流量流（切子视图时 onDeactivated 停止，切回时重启；startTrafficStream 有防重入守卫）。
  onActivated(() => {
    viewActive.value = true
    checkUpdate()
    // 订阅二级界面期间切换激活订阅 / 更新激活订阅：节点列表已失效，回到本视图补拉
    if (nodesDirty && coreStatus.value.downloaded) {
      nodesDirty = false
      void loadProxies()
    }
    // 设置子视图可能已完全卸载（跨视图状态对账）：重激活拉权威核心状态，
    // 核心已删则清残留节点列表（enabled 已由 proxy-enabled 事件同步为 false）
    void loadCoreStatus().then(() => {
      if (!coreStatus.value.downloaded) {
        nodesDirty = false
        proxiesData.value = null
        selectedNodeName.value = ''
        delayMap.value = {}
      }
    })
    if (isEnabled.value) startTrafficStream()
  })

  // 切子视图（连接/规则/日志）时被 KeepAlive 缓存：停流量流免空转。
  onDeactivated(() => {
    viewActive.value = false
    stopTrafficStream()
  })

  onUnmounted(() => {
    unlistenProgress?.()
    unlistenReady?.()
    unlistenEnabled?.()
    unlistenMode?.()
    unlistenStatus?.()
    unlistenSubUpdated?.()
    stopTrafficStream()
  })

  return {
    items,
    selectedIndex,
    baseListRef,
    onExecute,
    hasSelectedNode,
    locateSelected,
    testing,
    nodes,
    testAll,
    coreStatus,
    isDownloading,
    traffic,
    trafficDownHistory,
    trafficUpHistory,
    coreError,
    updateInfo,
    isEnabled,
    toggling,
    statusLoaded,
    reconnect,
    updateCore,
    downloadCore,
    downloadText,
    toggleEnabled,
    config,
    MODE_OPTIONS,
    onModeChange,
    modeSelectRef,
    groupSelectRef,
    groupOptions,
    activeGroupName,
    onGroupChange,
    delayColor,
    formatDelay,
    activeSubscription,
    activeSubscriptionTitle,
    activeSubscriptionSubtitle,
    fmtTrafficRate,
  }
}
