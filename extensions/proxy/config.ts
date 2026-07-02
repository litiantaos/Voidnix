import { watch } from 'vue'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

/// 正式版默认端口（mihomo/clash 惯例值）。
const PROD_MIXED_PORT = 7890
const PROD_CONTROLLER_PORT = 9090
/// dev 版默认端口（+1 偏移）：mihomo 以 root 常驻，app 退出后仍占端口，
/// dev/prod 同默认端口必然冲突（后启动者 controller bind 失败 → wait_ready 超时）。
/// 偏移让两版真正并存（与快捷键 Shift 叠加同理）。
const DEV_MIXED_PORT = PROD_MIXED_PORT + 1
const DEV_CONTROLLER_PORT = PROD_CONTROLLER_PORT + 1

const isDev = import.meta.env.DEV

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
/// 统一 TUN 模式（root mihomo 常驻 + 热重载 active/idle），无模式开关。此处仅保存用户偏好与订阅元数据。
export const config = defineConfig('extensions/proxy/config', {
  /// 规则模式：rule | global（关闭代理走 idle direct，非用户可选）。
  mode: 'rule' as 'rule' | 'global',
  /// 混合代理端口（HTTP/SOCKS5 共用）。
  mixedPort: isDev ? DEV_MIXED_PORT : PROD_MIXED_PORT,
  /// mihomo external-controller 端口。
  controllerPort: isDev ? DEV_CONTROLLER_PORT : PROD_CONTROLLER_PORT,
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

/// dev 版端口隔离迁移：旧 config.json 可能沿用 prod 默认端口（7890/9090），
/// 与正式版 root mihomo 常驻进程冲突。defineConfig 异步 backfill 把磁盘旧默认值
/// 覆盖到 reactive 后，此 watch 检测到并迁移至 dev 默认值（+1），随后自动持久化。
/// 用户自定义端口（非 prod 默认）不受影响；dev 版端口为 prod 默认值必然冲突，此约束合理。
if (isDev) {
  watch(
    () => [config.mixedPort, config.controllerPort] as const,
    ([mp, cp]) => {
      if (mp === PROD_MIXED_PORT) config.mixedPort = DEV_MIXED_PORT
      if (cp === PROD_CONTROLLER_PORT) config.controllerPort = DEV_CONTROLLER_PORT
    },
    { flush: 'post' },
  )
}
