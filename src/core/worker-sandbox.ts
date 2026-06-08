/**
 * Tier 2 Worker 沙箱：创建 Web Worker，注入扩展代码和 host API proxy，
 * 通过 JSON-RPC 与宿主通信。Worker 运行在 CSP 沙箱内，无 DOM/网络权限。
 */

import { invoke } from '@tauri-apps/api/core'
import { load } from '@tauri-apps/plugin-store'
import { copyAndHide } from '@/utils/clipboard'

interface WorkerMessage {
  id?: string
  result?: unknown
  error?: string
  method?: string
  params?: unknown[]
  kind?: 'ready' | 'rpc'
}

const activeWorkers = new Map<string, Worker>()
const pendingCalls = new Map<
  string,
  { resolve: (v: unknown) => void; reject: (e: Error) => void }
>()
const extensionStores = new Map<string, Promise<import('@tauri-apps/plugin-store').Store>>()

export function terminateWorker(extId: string) {
  const worker = activeWorkers.get(extId)
  if (worker) {
    worker.terminate()
    activeWorkers.delete(extId)
  }
  extensionStores.delete(extId)
  for (const [callId, call] of pendingCalls) {
    if (callId.startsWith(extId + ':')) {
      call.reject(new Error('Worker terminated'))
      pendingCalls.delete(callId)
    }
  }
}

/** 启动 Tier 2 扩展 Worker */
export async function spawnExtension(id: string): Promise<{
  extId: string
  invoke: (method: string, ...params: unknown[]) => Promise<unknown>
}> {
  terminateWorker(id)

  const rawCode = await invoke<string>('ext_entry_content', { id })
  if (!rawCode) throw new Error(`Extension not found: ${id}`)

  const { setupCode, moduleBody } = parseExtensionCode(rawCode)

  const workerCode = buildWorkerBootstrap(moduleBody, setupCode)
  const blob = new Blob([workerCode], { type: 'application/javascript' })
  const blobUrl = URL.createObjectURL(blob)
  const worker = new Worker(blobUrl)
  URL.revokeObjectURL(blobUrl)

  activeWorkers.set(id, worker)

  // 等待 worker 就绪
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Worker init timeout')), 5000)
    const handler = (e: MessageEvent<WorkerMessage>) => {
      if (e.data.kind === 'ready') {
        clearTimeout(timeout)
        worker.removeEventListener('message', handler)
        resolve()
      }
    }
    worker.addEventListener('message', handler)
  })

  // 处理 Worker 返回的消息
  worker.addEventListener('message', (e: MessageEvent<WorkerMessage>) => {
    const data = e.data

    // Worker 请求宿主执行 host API
    if (data.kind === 'rpc' && data.method) {
      handleHostRPC(data.method, data.params || [], data.id || '', id, worker)
    }
  })

  // 返回扩展调用接口
  const extInvoke = (method: string, ...params: unknown[]): Promise<unknown> => {
    return new Promise((resolve, reject) => {
      const callId = `${id}:${method}:${Date.now()}:${Math.random().toString(36).slice(2)}`
      pendingCalls.set(callId, { resolve, reject })

      setTimeout(() => {
        if (pendingCalls.has(callId)) {
          pendingCalls.delete(callId)
          reject(new Error(`RPC timeout: ${method}`))
        }
      }, 30000)

      const handler = (e: MessageEvent<WorkerMessage>) => {
        if (e.data.id === callId) {
          worker.removeEventListener('message', handler)
          pendingCalls.delete(callId)
          if (e.data.error) reject(new Error(e.data.error))
          else resolve(e.data.result)
        }
      }
      worker.addEventListener('message', handler)
      worker.postMessage({ method, params, id: callId })
    })
  }

  return { extId: id, invoke: extInvoke }
}

/** 获取扩展专属 Store（惰性创建，按扩展 ID 隔离） */
function getExtStore(extId: string): Promise<import('@tauri-apps/plugin-store').Store> {
  let promise = extensionStores.get(extId)
  if (promise) return promise
  promise = load(`extensions/${extId}/storage.json`)
  extensionStores.set(extId, promise)
  return promise
}

/** 宿主处理 Worker 的 host API 请求 */
async function handleHostRPC(
  method: string,
  params: unknown[],
  callId: string,
  _extId: string,
  worker: Worker,
) {
  try {
    let result: unknown

    switch (method) {
      case 'ctx.ui.hide':
        result = await invoke('hide_window', { auto: false })
        break
      case 'ctx.ui.setView':
        window.dispatchEvent(
          new CustomEvent('worker-view', { detail: { extId: _extId, view: params[0] } }),
        )
        result = undefined
        break
      case 'ctx.clipboard.write':
        await copyAndHide(params[0] as string)
        result = undefined
        break
      case 'ctx.http.fetch': {
        const [url, init] = params as [string, RequestInit?]
        const response = await fetch(url, init)
        const text = await response.text()
        result = { status: response.status, body: text }
        break
      }
      case 'ctx.storage.get': {
        const store = await getExtStore(_extId)
        result = (await store.get(params[0] as string)) ?? null
        break
      }
      case 'ctx.storage.set': {
        const store = await getExtStore(_extId)
        await store.set(params[0] as string, params[1])
        await store.save()
        result = undefined
        break
      }
      default:
        result = undefined
    }

    worker.postMessage({ id: callId, result })
  } catch (error) {
    worker.postMessage({ id: callId, error: (error as Error).message })
  }
}

/** 解析扩展代码：分离 setup 代码和 export default 模块体 */
function parseExtensionCode(rawCode: string): { setupCode: string; moduleBody: string } {
  const match = rawCode.match(/^([\s\S]*?)export\s+default\s+([\s\S]*)$/m)
  if (match) {
    return {
      setupCode: match[1].trim(),
      moduleBody: match[2].trim().replace(/;\s*$/, ''),
    }
  }
  return { setupCode: '', moduleBody: rawCode.trim().replace(/;\s*$/, '') }
}

/** 构建 Worker 代码 */
function buildWorkerBootstrap(moduleBody: string, setupCode?: string): string {
  const setup = setupCode ? `\n// ── Extension setup code ───────────────────\n${setupCode}\n` : ''
  return `
// Voidnix Tier 2 Worker Sandbox
${setup}
var __extension = ${moduleBody};

function hostRPC(method, params) {
  return new Promise(function(resolve, reject) {
    var id = Math.random().toString(36).slice(2) + Date.now().toString(36);
    var handler = function(e) {
      if (e.data.id === id) {
        self.removeEventListener('message', handler);
        if (e.data.error) reject(new Error(e.data.error));
        else resolve(e.data.result);
      }
    };
    self.addEventListener('message', handler);
    self.postMessage({ kind: 'rpc', method: method, params: params, id: id });
  });
}

var ctx = {
  ui: {
    hide: function() { return hostRPC('ctx.ui.hide', []); },
    setView: function(view) { return hostRPC('ctx.ui.setView', [view]); }
  },
  clipboard: {
    write: function(text) { return hostRPC('ctx.clipboard.write', [text]); }
  },
  storage: {
    get: function(key) { return hostRPC('ctx.storage.get', [key]); },
    set: function(key, value) { return hostRPC('ctx.storage.set', [key, value]); }
  },
  http: {
    fetch: function(url, init) { return hostRPC('ctx.http.fetch', [url, init]); }
  }
};

// ── Message handler ─────────────────────────────
self.onmessage = async function(event) {
  var method = event.data.method;
  var params = event.data.params || [];
  var id = event.data.id;

  if (!method || id === undefined) return;

  try {
    var fn = __extension[method];
    if (typeof fn !== 'function') {
      self.postMessage({ id: id, error: 'Method not found: ' + method });
      return;
    }
    var result = await fn.apply(__extension, params.concat([ctx]));
    self.postMessage({ id: id, result: result });
  } catch (error) {
    self.postMessage({ id: id, error: error.message || String(error) });
  }
};

// ── Notify host ──────────────────────────────────
self.postMessage({ kind: 'ready', id: __extension.id || 'unknown' });
`
}
