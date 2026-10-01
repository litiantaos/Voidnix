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
  GATEWAY_PORT,
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
  try {
    gatewayStatus.value = await invoke<GatewayStatus>(CMD.aiGatewaySync, {
      enabled: config.enabled,
      routes,
    })
  } catch (e) {
    console.error('[ai-gateway] sync failed:', e)
  }

  // CC 接线独立于网关开关:ccTakeover 是用户意愿(默认关,不擅改 CC 配置文件),
  // 实际接管还需网关在跑(enabled)且存在可接线载荷;任一缺失(关接管/关网关/删光
  // Anthropic 端点)且处于接管态即还原。接管态唯一真相源 = Rust 侧快照存在性(ccManaged),
  // 不另设本地标志
  const models = routableModels(routes)
  const payload =
    config.enabled && config.ccTakeover
      ? buildCcPayload(
          routes,
          {
            sonnet: effectiveAlias(models, config.sonnet),
            haiku: effectiveAlias(models, config.haiku),
          },
          gatewayStatus.value?.port ?? GATEWAY_PORT,
          config.cc1mContext,
        )
      : null
  if (payload) {
    try {
      await invoke(CMD.aiGatewayCcApply, { payload })
      ccApplyError.value = null
      if (gatewayStatus.value) gatewayStatus.value.ccManaged = true
    } catch (e) {
      ccApplyError.value = String(e)
      console.error('[ai-gateway] cc apply failed:', e)
    }
  } else if (gatewayStatus.value?.ccManaged) {
    try {
      await invoke(CMD.aiGatewayCcRemove)
      ccApplyError.value = null
      if (gatewayStatus.value) gatewayStatus.value.ccManaged = false
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
