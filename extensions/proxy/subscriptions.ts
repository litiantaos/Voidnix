/// 订阅数据层：拉取回填、后台自动更新、激活订阅摘要与配置不变式。
/// 主视图消费入口摘要 / 节点失效联动，订阅二级界面消费拉取与自动更新；
/// 本模块不持有视图状态（弹窗/选中态归 useSubscriptions）。

import { computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { t } from '@/runtime/i18n'
import { whenConfigReady } from '@/runtime/storage'
import { parseUtcMs } from '@/utils/datetime'
import { toErrorMessage } from '@/utils/format'
import { type Subscription, config, addSubscription, updateSubscription } from './config'

/// 拉取订阅并回填元数据（节点数/更新时间/到期时间），手动（弹窗保存/行内更新）与
/// 自动更新共用；expire 为 unix 秒（null = 订阅方未提供）；skipped 为通用格式转换
/// 跳过的行数（>0 时 UI 明示「跳过 M 行」，避免静默丢节点）。
export async function fetchSubscription(
  id: string,
  url: string,
): Promise<{ count: number; skipped: number }> {
  const res = await invoke<{ count: number; expire: number | null; skipped: number }>(
    CMD.proxyUpdateSubscription,
    { id, url },
  )
  updateSubscription(id, {
    proxyCount: res.count,
    updatedAt: new Date().toISOString(),
    expiresAt: res.expire ? new Date(res.expire * 1000).toISOString() : '',
    lastError: '', // 成功即清失败标记（自动更新恢复后行内警告消失）
  })
  return { count: res.count, skipped: res.skipped }
}

/// 更新成功文案：有跳过行时明示（通用格式里不支持的协议/垃圾行），否则常规节点数。
export function nodesUpdatedMessage(res: { count: number; skipped: number }): string {
  return res.skipped > 0
    ? t('proxy.nodesUpdatedSkipped', { count: res.count, skipped: res.skipped })
    : t('proxy.nodesUpdated', { count: res.count })
}

// ── 订阅自动更新（模块级，app 启动即生效）──
// 阈值：上次拉取超过 autoUpdateIntervalHours（或从未拉取成功）视为过期，每小时复查一次
// （启动时立即查；复查频率与用户设置的更新间隔解耦，间隔由阈值表达）。后台静默更新：
// 失败仅 console（机场偶发不可达，toast 是噪音）；启动时离线等场景由每小时复查自愈。
// 激活订阅被更新后节点列表经 proxy-subscription-updated 事件驱动刷新（见 useProxyPanel），
// 核心未运行时 Rust 侧 reload 为安全空操作。
const AUTO_UPDATE_CHECK_MS = 60 * 60 * 1000
let autoUpdateInFlight = false

/// 订阅是否超过自动更新间隔（含从未拉取成功）。间隔 clamp 防御磁盘手改非法值
/// （0/负数会让每次复查都触发拉取；NaN 落 24h 档）。导出供单测。
export function isSubscriptionStale(s: Subscription): boolean {
  if (!s.url) return false
  const hours = Number.isFinite(config.autoUpdateIntervalHours)
    ? Math.max(1, config.autoUpdateIntervalHours)
    : 24
  const t = parseUtcMs(s.updatedAt)
  // updatedAt 空/非法（从未拉取成功，如编辑保存时网络失败）同样视为过期，后台自动补拉
  return Number.isNaN(t) || Date.now() - t >= hours * 60 * 60 * 1000
}

async function autoUpdateSubscriptions(): Promise<void> {
  if (!config.autoUpdateEnabled || autoUpdateInFlight) return
  const stale = config.subscriptions.filter(isSubscriptionStale)
  if (stale.length === 0) return
  autoUpdateInFlight = true
  try {
    // 串行拉取：对机场温和（部分按频率限流），逐条失败互不影响；
    // 每轮复查开关（关闭即时止血，已发起的单次拉取跑完即止）
    for (const s of stale) {
      if (!config.autoUpdateEnabled) break
      try {
        await fetchSubscription(s.id, s.url)
      } catch (e) {
        console.error('[proxy] auto update subscription failed:', e)
        // 失败曾只进 console：后台静默路径用户无从知晓订阅已陈旧。落盘供订阅行标记
        updateSubscription(s.id, { lastError: toErrorMessage(e, t('proxy.updateFailed')) })
      }
    }
  } finally {
    autoUpdateInFlight = false
  }
}

if (isTauri) {
  // 等配置回填后再查（模块加载时 subscriptions 仍是默认空项）；timer 随页面文档
  // 生命周期存活（navigate 重载后旧 timer 随旧文档销毁，模块重执行重建）
  void whenConfigReady('extensions/proxy/config').then(() => {
    void autoUpdateSubscriptions()
    setInterval(() => void autoUpdateSubscriptions(), AUTO_UPDATE_CHECK_MS)
  })
  // 设置变化即时生效：开启/缩短间隔立即按新阈值复查（等一小时才反应不符合直觉）
  watch(
    () => [config.autoUpdateEnabled, config.autoUpdateIntervalHours] as const,
    () => {
      if (config.autoUpdateEnabled) void autoUpdateSubscriptions()
    },
  )
}

/// 当前激活订阅（activeSubscriptionId 失效时回退首项，normalizer 落定前的过渡态）：
/// 主视图入口行摘要的单一取数口径。
export const activeSubscription = computed<Subscription | null>(() => {
  const subs = config.subscriptions
  return subs.find((s) => s.id === config.activeSubscriptionId) ?? subs[0] ?? null
})

// 兜底：订阅列表至少一项（磁盘旧值可能为空数组，覆盖默认项；类似 agent 默认 provider 始终存在）
watch(
  () => config.subscriptions.length,
  (len) => {
    if (len === 0) addSubscription('', '')
  },
  { immediate: true },
)

// 激活订阅归一化：activeSubscriptionId 失效（空 / 指向已删订阅）时回退首项，
// 保证前端与 Rust（build_run_config）始终对同一有效 id 达成共识。
function normalizeActiveSubscription() {
  const ids = config.subscriptions.map((s) => s.id)
  if (!ids.includes(config.activeSubscriptionId)) {
    config.activeSubscriptionId = ids[0] ?? ''
  }
}

// 初始归一化必须等配置回填：defineConfig backfill 以 deepEqual(当前值, 默认值) 判定
// 是否写入磁盘值——回填前改写 activeSubscriptionId（默认 ''）会让守卫失配，磁盘里
// 用户上次激活的订阅被静默丢弃（回退首项）。Tauri 下挂到 ready；浏览器预览无持久化，
// 直接归一化。
if (isTauri) {
  void whenConfigReady('extensions/proxy/config').then(normalizeActiveSubscription)
} else {
  normalizeActiveSubscription()
}
watch(
  () => config.subscriptions.map((s) => s.id).join('\0') + '\0' + config.activeSubscriptionId,
  normalizeActiveSubscription,
)
