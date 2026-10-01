import { invoke } from '@tauri-apps/api/core'
import { ref, watch } from 'vue'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { config as hub } from '@/runtime/ai-providers'
import { whenConfigReady } from '@/runtime/storage'
import { config } from './config'
import {
  buildRoutes,
  routableModels,
  buildCcPayload,
  fallbackAliases,
  effectiveAlias,
  type GatewayRoute,
} from './logic'

/** Rust `StatusReport` 同构(camelCase) */
export interface GatewayStatus {
  running: boolean
  port: number
  routeCount: number
  bindError: string | null
  ccManaged: boolean
}

/** 视图共享状态:push 后即时刷新,避免等下一次 invoke */
export const gatewayStatus = ref<GatewayStatus | null>(null)

/** CC 接管/还原失败原因(null = 无错误);apply 抛错(settings.json 非法 JSON 等)须在 UI 可见,不得静默 */
export const ccApplyError = ref<string | null>(null)

let syncTimer: ReturnType<typeof setTimeout> | null = null

/** 路由表派生(中枢 → 网关;View 状态页共用) */
export function currentRoutes(): GatewayRoute[] {
  return buildRoutes(hub.providers)
}

async function push() {
  const routes = currentRoutes()
  let st: GatewayStatus
  try {
    st = await invoke<GatewayStatus>(CMD.aiGatewaySync, {
      enabled: config.enabled,
      routes,
    })
    gatewayStatus.value = st
  } catch (e) {
    // 状态未知本轮不动 CC 接线:误 apply 会把 CC 接到死端口,误 remove 会无谓
    // 打断在用的接管,两者都不如下一轮 sync 再决策
    console.error('[ai-gateway] sync failed:', e)
    return
  }

  // CC 接线独立于网关开关:ccTakeover 是用户意愿(默认关,不擅改 CC 配置文件),
  // 实际接管还需网关真正在跑(enabled 且 bind 成功——绑定失败时写入会把 CC 指向
  // 死端口或占用端口的陌生进程)且存在可接线载荷;任一失守(关接管/关网关/绑定
  // 失败/删光 Anthropic 端点)且处于接管态即还原。接管态唯一真相源 = Rust 侧快照
  // 存在性(ccManaged),不另设本地标志
  const gatewayOk = st.running && !st.bindError
  const models = routableModels(routes)
  const payload =
    config.enabled && config.ccTakeover && gatewayOk
      ? buildCcPayload(
          routes,
          {
            sonnet: effectiveAlias(models, config.sonnet),
            haiku: effectiveAlias(models, config.haiku),
          },
          st.port,
          config.cc1mContext,
        )
      : null
  if (payload) {
    try {
      await invoke(CMD.aiGatewayCcApply, { payload })
      ccApplyError.value = null
      st.ccManaged = true
    } catch (e) {
      ccApplyError.value = String(e)
      console.error('[ai-gateway] cc apply failed:', e)
    }
  } else if (st.ccManaged) {
    try {
      await invoke(CMD.aiGatewayCcRemove)
      ccApplyError.value = null
      st.ccManaged = false
    } catch (e) {
      ccApplyError.value = String(e)
      console.error('[ai-gateway] cc remove failed:', e)
    }
  }
}

/** 冷路径:接管开启但两档别名全空时落兜底(用户此后可改;单项失效不静默重写) */
function ensureAliasDefaults() {
  if (!config.enabled || !config.ccTakeover) return
  if (config.sonnet || config.haiku) return
  const fb = fallbackAliases(routableModels(currentRoutes()))
  config.sonnet = fb.sonnet
  config.haiku = fb.haiku
}

function schedulePush() {
  if (syncTimer) clearTimeout(syncTimer)
  syncTimer = setTimeout(() => {
    syncTimer = null
    void push()
  }, 400)
}

/** 扩展 setup:配置就绪 → 别名兜底 → 首推(sync 响应即完整状态,含接管标记)→ deep watch。 */
export async function setupAiGatewaySync() {
  if (!isTauri) return
  await Promise.all([
    whenConfigReady('config/ai-providers'),
    whenConfigReady('extensions/ai-gateway/config'),
  ])
  ensureAliasDefaults()
  await push()
  watch(
    [() => hub.providers, config],
    () => {
      ensureAliasDefaults()
      schedulePush()
    },
    { deep: true },
  )
}
