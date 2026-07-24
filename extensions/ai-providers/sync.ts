import { invoke } from '@tauri-apps/api/core'
import { load } from '@tauri-apps/plugin-store'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import {
  config,
  registerSyncHandler,
  refreshEnvSnapshot,
  normalizeProvidersInPlace,
  addAiProvider,
  getProviderById,
  type AiProvider,
} from '@/runtime/ai-providers'
import { whenConfigReady } from '@/runtime/storage'
import { buildExportPayload } from './logic'

let syncTimer: ReturnType<typeof setTimeout> | null = null
let lastExportPath = ''

export function getLastExportPath(): string {
  return lastExportPath
}

async function doExport() {
  if (!isTauri) return
  const { envText } = buildExportPayload({
    providers: config.providers as AiProvider[],
  })
  try {
    lastExportPath = await invoke<string>(CMD.aiProvidersExport, { envText })
  } catch (e) {
    console.error('[ai-providers] export failed:', e)
  }
}

function scheduleExport() {
  if (syncTimer) clearTimeout(syncTimer)
  syncTimer = setTimeout(() => {
    syncTimer = null
    void doExport()
  }, 400)
}

/**
 * 一次性：从旧 agent/translate 磁盘 config 导入提供商到中枢。
 * 仅中枢为空时执行；成功后删除旧字段以免密钥滞留。
 */
async function importLegacyProviders(): Promise<number> {
  if (!isTauri || config.providers.length > 0) return 0
  let imported = 0

  // agent：aiProviders: { id, endpoint, apiKey, models }[]
  try {
    const store = await load('extensions/agent/config.json')
    const raw = await store.get<unknown>('aiProviders')
    if (Array.isArray(raw)) {
      for (const item of raw) {
        if (!item || typeof item !== 'object') continue
        const r = item as Record<string, unknown>
        const endpoint = typeof r.endpoint === 'string' ? r.endpoint.trim() : ''
        const apiKey = typeof r.apiKey === 'string' ? r.apiKey : ''
        const models = Array.isArray(r.models) ? r.models.map(String) : []
        if (!endpoint && !apiKey.trim() && models.every((m) => !String(m).trim())) continue
        const id = typeof r.id === 'string' ? r.id : undefined
        if (id && getProviderById(id)) continue
        if (endpoint && config.providers.some((p) => p.endpoint.trim() === endpoint)) continue
        addAiProvider({ id, endpoint, apiKey, models })
        imported += 1
      }
      if (imported > 0) {
        try {
          await store.delete('aiProviders')
          await store.save()
        } catch {
          /* 删旧字段失败不阻断 */
        }
      }
    }
  } catch (e) {
    console.warn('[ai-providers] legacy agent import skipped:', e)
  }

  // translate：configs 中 type=ai 且仍带 endpoint/apiKey
  try {
    const store = await load('extensions/translate/config.json')
    const raw = await store.get<unknown>('configs')
    if (Array.isArray(raw)) {
      let touched = false
      for (const item of raw) {
        if (!item || typeof item !== 'object') continue
        const r = item as Record<string, unknown>
        if (r.type !== 'ai') continue
        const endpoint = typeof r.endpoint === 'string' ? r.endpoint.trim() : ''
        const apiKey = typeof r.apiKey === 'string' ? r.apiKey : ''
        const models = Array.isArray(r.models) ? r.models.map(String) : []
        if (!endpoint && !apiKey.trim()) continue
        if (endpoint && config.providers.some((p) => p.endpoint.trim() === endpoint)) {
          touched = true
          continue
        }
        addAiProvider({ endpoint, apiKey, models })
        imported += 1
        touched = true
      }
      // 旧密钥字段留在 configs 对象里；消费者侧 migrate 会 strip
      if (touched) {
        /* agent 侧已 delete；translate 由 config 扩展 strip 响应式字段 */
      }
    }
  } catch (e) {
    console.warn('[ai-providers] legacy translate import skipped:', e)
  }

  if (imported > 0) {
    console.warn(`[ai-providers] imported ${imported} provider(s) from legacy consumer configs`)
  }
  return imported
}

/** 中枢 setup：规范化 + 遗留导入 + env 快照 + 导出钩子。 */
export async function setupAiProvidersSync() {
  await whenConfigReady('config/ai-providers')
  normalizeProvidersInPlace()
  await importLegacyProviders()
  await refreshEnvSnapshot()
  // registerSyncHandler 会立即调一次 handler，无需再 scheduleExport
  registerSyncHandler(scheduleExport)
}
