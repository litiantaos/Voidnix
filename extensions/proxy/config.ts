import { watch } from 'vue'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

/// 订阅源（Clash YAML）。url 由 Rust 端拉取解析，proxyCount/updatedAt 在拉取成功后回填。
export interface Subscription {
  id: string
  name: string
  url: string
  updatedAt: string
  proxyCount: number
}

/// prod 默认端口（行业默认，与其他工具兼容）。
export const DEFAULT_MIXED_PORT = 7890
export const DEFAULT_CONTROLLER_PORT = 9090

/// dev 构建端口偏移 +1：dev/prod 默认端口隔离，两个 mihomo 可同时常驻互不干扰。
/// import.meta.env.DEV 在 tauri:dev（debug 构建 + dev bundle id）时 true，release 构建 false。
const DEV_PORT_OFFSET = import.meta.env.DEV ? 1 : 0

/// proxy 扩展自管配置（持久化至 extensions/proxy/config.json）。
/// 注意：代理核心是否运行（enabled）为 Rust 状态型（set/is_proxy_enabled），不存此；
/// 统一 TUN 模式（root mihomo 常驻 + 热重载 active/idle），无模式开关。此处仅保存用户偏好与订阅元数据。
export const config = defineConfig('extensions/proxy/config', {
  /// 规则模式：rule | global（关闭代理走 idle direct，非用户可选）。
  mode: 'rule' as 'rule' | 'global',
  /// 混合代理端口（HTTP/SOCKS5 共用）。dev 构建 +1 偏移，与 prod 端口隔离。
  mixedPort: DEFAULT_MIXED_PORT + DEV_PORT_OFFSET,
  /// mihomo external-controller 端口。dev 构建 +1 偏移。
  controllerPort: DEFAULT_CONTROLLER_PORT + DEV_PORT_OFFSET,
  /// mihomo API bearer token（空则核心启动时随机生成并回写）。
  secret: '',
  /// 订阅源集合（默认一项空订阅，类似 agent 默认 provider，保证列表始终非空可编辑）。
  subscriptions: [
    { id: generateRequestId(), name: '', url: '', updatedAt: '', proxyCount: 0 },
  ] as Subscription[],
  /// 当前激活订阅 id：同一时刻仅一个订阅生效，build_run_config 仅合并此订阅的 YAML。
  /// 空 = 无激活（回退首项）；由 normalizer watch 保证始终指向有效 id。
  activeSubscriptionId: '',
})

/// 端口变体归一化（模块级，app 启动即生效）。
///
/// config.json 可能残留对端变体默认端口（历史污染 / 手动复制 / defineConfig 异步
/// backfill 覆盖正确默认值）。此处用 `flush: 'sync'` 确保端口变更（含 backfill 回填）
/// 在同一同步执行栈内即时修正——消除 backfill 写入错误值到 normalizer 修正之间的窗口。
/// Rust 侧 `correct_variant_ports`（cfg!(debug_assertions)）作权威兜底。
watch(
  () => [config.mixedPort, config.controllerPort] as const,
  () => {
    if (import.meta.env.DEV) {
      if (config.mixedPort === DEFAULT_MIXED_PORT) config.mixedPort = DEFAULT_MIXED_PORT + 1
      if (config.controllerPort === DEFAULT_CONTROLLER_PORT)
        config.controllerPort = DEFAULT_CONTROLLER_PORT + 1
    } else {
      if (config.mixedPort === DEFAULT_MIXED_PORT + 1) config.mixedPort = DEFAULT_MIXED_PORT
      if (config.controllerPort === DEFAULT_CONTROLLER_PORT + 1)
        config.controllerPort = DEFAULT_CONTROLLER_PORT
    }
  },
  { immediate: true, flush: 'sync' },
)

/// 模式值（rule=规则分流，global=全局代理）
export const MODE_VALUES = ['rule', 'global'] as const
export type ProxyMode = (typeof MODE_VALUES)[number]

/// CRUD helpers（defineConfig reactive 数组变更自动持久化）
export function addSubscription(name = '', url = ''): string {
  const id = generateRequestId()
  config.subscriptions.push({ id, name, url, updatedAt: '', proxyCount: 0 })
  return id
}

export function updateSubscription(id: string, partial: Partial<Omit<Subscription, 'id'>>) {
  const s = config.subscriptions.find((s) => s.id === id)
  if (s) Object.assign(s, partial)
}

export function removeSubscription(id: string) {
  const idx = config.subscriptions.findIndex((s) => s.id === id)
  if (idx === -1) return
  config.subscriptions.splice(idx, 1)
}
