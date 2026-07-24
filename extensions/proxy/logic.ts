/// proxy 扩展纯逻辑：延迟着色、节点过滤、模式标签、分组解析。

/// mihomo controller /proxies 响应类型
export interface ProxyHistory {
  time: string
  delay: number
}

export interface ProxyEntry {
  name: string
  type: string // Selector | URLTest | Fallback | ss | vmess | trojan | Direct | ...
  now?: string // selector 当前选中
  all?: string[] // selector/url-test 成员名列表
  history?: ProxyHistory[]
}

export interface ProxiesResponse {
  proxies: Record<string, ProxyEntry>
}

/// 流式测速单帧（Channel 推送，与 Rust DelayResult 对应）：delay=0 表示测速失败/超时。
export interface DelayResult {
  name: string
  delay: number
}

/// 是否为用户可手动选择的 selector 分组（排除 mihomo 隐式 GLOBAL）。
export function isUserSelectorGroup(node: ProxyEntry): boolean {
  return node.type === 'Selector' && node.name !== 'GLOBAL'
}

/// 节点最新延迟：优先测速缓存，回退 history 末项。
export function latestDelay(history?: ProxyHistory[]): number {
  if (!history || history.length === 0) return 0
  return history[history.length - 1]?.delay ?? 0
}

/// 测速超时哨兵值：测速失败/超时写入 delayMap，与「未测速」（0）区分。
export const DELAY_TIMEOUT = -1

/// 延迟（ms）→ 颜色语义类。连通=绿，连不通=红，未测速（0）返回空串不着色。
export function delayColor(ms: number): string {
  if (ms === DELAY_TIMEOUT) return 'text-danger'
  if (ms > 0) return 'text-success'
  return ''
}

/// 延迟 → 显示文本（超时显示「超时」，未测速返回空串不占位）
export function formatDelay(ms: number | null | undefined): string {
  if (ms === DELAY_TIMEOUT) return '超时'
  if (ms == null || ms <= 0) return ''
  return `${ms}ms`
}

/// 节点列表按名称过滤（不区分大小写、子串匹配）
export function filterNodes<T extends { name: string }>(nodes: T[], query: string): T[] {
  const q = query.trim().toLowerCase()
  if (!q) return nodes
  return nodes.filter((n) => n.name.toLowerCase().includes(q))
}
