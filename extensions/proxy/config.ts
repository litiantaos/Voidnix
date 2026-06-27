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

/// proxy 扩展自管配置（持久化至 extensions/proxy/config.json）。
/// 注意：代理核心是否运行（enabled）为 Rust 状态型（set/is_proxy_enabled），不存此；
/// 系统代理不再独立配置——user 模式自动应用，TUN 模式无需（见 native/mod.rs）。
/// 此处仅保存用户偏好与订阅元数据。
export const config = defineConfig('extensions/proxy/config', {
  /// TUN 模式偏好（需 root 提权，Phase 7 接入）。
  tunMode: false,
  /// 规则模式：rule | global | direct。
  mode: 'rule' as 'rule' | 'global' | 'direct',
  /// 混合代理端口（HTTP/SOCKS5 共用）。
  mixedPort: 7890,
  /// mihomo external-controller 端口。
  controllerPort: 9090,
  /// mihomo API bearer token（空则核心启动时随机生成并回写）。
  secret: '',
  /// 订阅源集合（默认一项空订阅，类似 agent 默认 provider，保证列表始终非空可编辑）。
  subscriptions: [
    { id: generateRequestId(), name: '', url: '', updatedAt: '', proxyCount: 0 },
  ] as Subscription[],
})

/// 规则模式选项
export const MODE_OPTIONS = [
  { label: '规则', value: 'rule' as const },
  { label: '全局', value: 'global' as const },
  { label: '直连', value: 'direct' as const },
]

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
