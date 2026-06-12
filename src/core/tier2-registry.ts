import { markRaw, reactive, h, type Component } from 'vue'
import type { AppModule, SearchResult } from '@/types/module'
import type { Manifest } from '@/types/ext-manifest'
import type { View, ListItem } from '@/types/declarative'
import { registerModule, getModule } from '@/core/module-registry'
import { spawnExtension, terminateWorker } from '@/core/worker-sandbox'
import { copyAndHide } from '@/utils/clipboard'
import DeclarativeHost from '@/components/declarative/DeclarativeHost.vue'

export interface Tier2Extension {
  manifest: Manifest
  isActive: boolean
}

interface WorkerHandle {
  extId: string
  invoke: (method: string, ...params: unknown[]) => Promise<unknown>
}

const extensions = new Map<string, Tier2Extension>()
const workers = new Map<string, WorkerHandle>()
const viewStates = new Map<string, View | null>()

export function getTier2Extension(id: string): Tier2Extension | undefined {
  return extensions.get(id)
}

export function getAllTier2Extensions(): Tier2Extension[] {
  return Array.from(extensions.values())
}

export function unregisterTier2(id: string): void {
  extensions.delete(id)
  workers.delete(id)
  viewStates.delete(id)
  terminateWorker(id)
}

export async function loadTier2Extensions(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const manifests: Manifest[] = await invoke('ext_list')
    for (const m of manifests) {
      const id = m.extension.id
      if (getModule(id)) continue
      extensions.set(id, { manifest: m, isActive: true })
      registerModule(buildTier2Module(m))
    }
  } catch {
    // 非 Tauri 环境或加载失败
  }
}

async function ensureWorker(id: string): Promise<WorkerHandle> {
  const existing = workers.get(id)
  if (existing) return existing
  const handle = await spawnExtension(id)
  workers.set(id, handle)
  return handle
}

async function callWorkerMethod(
  id: string,
  method: string,
  ...params: unknown[]
): Promise<unknown> {
  const handle = await ensureWorker(id)
  return handle.invoke(method, ...params)
}

/**
 * 处理声明式视图上抛的 action。
 *
 * 默认语义：list 视图 item 未声明 actions 数组时，execute 表示"复制 title 并隐藏窗口"，
 * 由框架直接执行（避免每个 T2 扩展重复样板 onAction）。扩展可通过两种方式接管：
 *   1. 给 item 声明 actions 数组（DeclarativeList 会以上抛 primary action.id 代替 'execute'）
 *   2. 让 item.title 为空，框架回落到转发 worker onAction
 *
 * 其他 actionId 一律转发到 worker onAction。
 */
export async function handleTier2Action(
  actionId: string,
  payload: Record<string, unknown>,
  callWorker: (method: string, ...params: unknown[]) => Promise<unknown>,
): Promise<void> {
  if (actionId === 'execute') {
    const item = payload.item as ListItem | undefined
    const text = item?.title?.trim()
    if (item && !item.actions?.length && text) {
      await copyAndHide(text)
      return
    }
  }
  await callWorker('onAction', actionId, payload)
}

function buildTier2Module(manifest: Manifest): AppModule {
  const meta = manifest.extension
  const extId = meta.id

  const state = reactive<{ currentView: View | null; extId: string }>({
    currentView: null,
    extId,
  })

  const viewListener = (e: Event) => {
    const ce = e as CustomEvent<{ extId?: string; view: View }>
    if (ce.detail.extId === extId) {
      state.currentView = ce.detail.view
      viewStates.set(extId, ce.detail.view)
    }
  }

  const handleAction = (actionId: string, payload: Record<string, unknown>) =>
    handleTier2Action(actionId, payload, (method, ...params) =>
      callWorkerMethod(extId, method, ...params).catch((e) => {
        console.error(`[tier2] onAction error for ${extId}:`, e)
      }),
    )

  const Tier2View: Component = {
    name: 'Tier2View',
    setup() {
      return () =>
        h(DeclarativeHost, {
          view: state.currentView,
          onAction: handleAction,
        })
    },
  }

  const mod: AppModule = {
    id: extId,
    name: meta.name,
    description: meta.description || '',
    icon: meta.icon || 'i-ri-puzzle-line',
    keywords: meta.keywords || [],
    placeholder: manifest.ui?.search_placeholder || `在 ${meta.name} 中搜索`,
    disableSearchInput: manifest.ui?.disable_search_input ?? false,
    order: 9990,
    view: markRaw(Tier2View),

    onActivate: async () => {
      window.addEventListener('worker-view', viewListener)
      try {
        await callWorkerMethod(extId, 'onActivate')
      } catch {}
      try {
        const view = (await callWorkerMethod(extId, 'onSearch', '')) as View | null
        if (view) {
          state.currentView = view
          viewStates.set(extId, view)
        }
      } catch {}
    },

    onDeactivate: async () => {
      window.removeEventListener('worker-view', viewListener)
      try {
        await callWorkerMethod(extId, 'onDeactivate')
      } catch {}
    },

    onModuleSearch: async (query: string): Promise<SearchResult[]> => {
      try {
        const view = (await callWorkerMethod(extId, 'onSearch', query)) as View | null
        state.currentView = view
        viewStates.set(extId, view)
      } catch {}
      return []
    },

    onExecute: async (result: SearchResult) => {
      // 主搜索按 Enter 触发路径：声明式 list view 的"自动复制 title"语义不适用于此
      // （那是 list view 的 execute 默认行为，而非模块入口的语义）。直接转发 worker onAction，
      // 由扩展按既有 onAction 协议自行决定行为。
      await callWorkerMethod(extId, 'onAction', 'execute', {
        item: { id: result.id, title: result.title, subtitle: result.description },
      }).catch(() => {})
    },
  }

  return mod
}
