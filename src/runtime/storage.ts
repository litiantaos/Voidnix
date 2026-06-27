import { reactive, watch } from 'vue'
import { load, type Store } from '@tauri-apps/plugin-store'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauri } from '@/utils/tauri'

// store 实例缓存：避免每次防抖保存重新 load()。
// 缓存 Promise<Store> 而非 Store，并发 getStore 共享同一 in-flight load，避免重复加载。
// 失败时清缓存：避免单次 IO 抖动演变为永久故障（rejected Promise 被缓存）。
const storeCache = new Map<string, Promise<Store>>()

function getStore(storePath: string): Promise<Store> {
  let promise = storeCache.get(storePath)
  if (!promise) {
    promise = load(`${storePath}.json`)
    storeCache.set(storePath, promise)
    promise.catch(() => storeCache.delete(storePath))
  }
  return promise
}

/// 递归深度相等比较。替换原 JSON.stringify 实现：
/// - 顺序无关：{a,b} 与 {b,a} 视为相等
/// - NaN 安全：Object.is 已处理 NaN === NaN
function deepEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== typeof b) return false
  // 走到这里 a/b 要么都为 null（已被 Object.is 处理），要么都不为 null
  if (a === null || b === null) return false
  if (typeof a !== 'object') return false
  const aArr = Array.isArray(a)
  const bArr = Array.isArray(b)
  if (aArr !== bArr) return false
  if (aArr) {
    const aa = a as unknown[]
    const bb = b as unknown[]
    if (aa.length !== bb.length) return false
    return aa.every((v, i) => deepEqual(v, bb[i]))
  }
  const aObj = a as Record<string, unknown>
  const bObj = b as Record<string, unknown>
  const aKeys = Object.keys(aObj).sort()
  const bKeys = Object.keys(bObj).sort()
  if (aKeys.length !== bKeys.length) return false
  return aKeys.every((k, i) => k === bKeys[i] && deepEqual(aObj[k], bObj[k]))
}

/// 默认类型守卫：typeof + Array.isArray 严格匹配。
/// 对象 vs 数组互斥（避免磁盘数组赋给默认对象字段或反之）。
function defaultValidate(_key: string, value: unknown, defaultValue: unknown): boolean {
  if (Array.isArray(defaultValue)) return Array.isArray(value)
  if (defaultValue !== null && typeof defaultValue === 'object') {
    return value !== null && typeof value === 'object' && !Array.isArray(value)
  }
  return typeof value === typeof defaultValue
}

/// 创建响应式配置对象，自动从磁盘加载 + 变更自动持久化。
///
/// @param storePath plugin-store 路径（不含 .json 后缀），如 'extensions/clipboard/config'
///                  或 'config/settings'。最终落盘 <appDataDir>/<storePath>.json
/// @param defaults  默认值（深克隆，源对象不会被污染）
///
/// 加载语义：load() 异步，扩展 setup / 早期命令可能读到 defaults
/// （磁盘值尚未回填）。安全参数由 Rust clamp 兜底，UI 可能短暂显示 defaults。
///
/// 跨窗口同步：订阅 plugin-store onChange，其他窗口 set 自动同步本地 reactive。
///
/// 退出 flush：onCloseRequested 触发 pending saveTimer 立即落盘，避免防抖窗口内变更丢失。
///
/// schema 变更：自开发自用不维护迁移，改 schema 时手动删磁盘 config.json 即可。
export function defineConfig<T extends object>(storePath: string, defaults: T): T {
  const config = reactive(structuredClone(defaults)) as T
  const defaultKeys = Object.keys(defaults)
  let isLoading = true

  // 异步从磁盘加载已保存的值
  // 竞态保护：backfill 的 store.get 异步，返回前用户可能已改某 key。
  // 写入前 deepEqual 检查「当前值是否仍为 default」——若已非 default 说明用户已改，跳过覆盖。
  getStore(storePath)
    .then(async (store) => {
      await Promise.all(
        defaultKeys.map(async (key) => {
          const saved = await store.get<unknown>(key)
          if (saved === null || saved === undefined) return
          // 类型守卫：磁盘值类型不匹配则丢弃（防止手动编辑注入错误类型）
          if (!defaultValidate(key, saved, (defaults as Record<string, unknown>)[key])) return
          const cur = (config as Record<string, unknown>)[key]
          const def = (defaults as Record<string, unknown>)[key]
          if (deepEqual(cur, def)) {
            // biome-ignore lint: dynamic key assignment
            ;(config as Record<string, unknown>)[key] = saved
          }
        }),
      )
      isLoading = false
    })
    .catch((e) => {
      isLoading = false
      console.error(`[config:${storePath}] load failed:`, e)
    })

  // 变更自动持久化（deep watch + 防抖 300ms + isLoading 抑制启动期冗余写）
  let saveTimer: ReturnType<typeof setTimeout> | null = null
  async function flushSave() {
    if (saveTimer) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    try {
      const store = await getStore(storePath)
      for (const key of defaultKeys) {
        await store.set(key, (config as Record<string, unknown>)[key])
      }
      await store.save()
    } catch (e) {
      console.error(`[config:${storePath}] save failed:`, e)
    }
  }
  watch(
    config,
    () => {
      if (isLoading) return
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(flushSave, 300)
    },
    { deep: true },
  )

  // 退出 flush：防抖窗口内变更不丢失（仅 Tauri 环境）
  if (isTauri) {
    getCurrentWindow().onCloseRequested(async () => {
      if (saveTimer) await flushSave()
    })
  }

  // 跨窗口同步：订阅 onChange，其他窗口改值时本地 reactive 自动同步
  getStore(storePath)
    .then((store) =>
      store.onChange<unknown>((key, value) => {
        if (isLoading) return
        if (!key || !defaultKeys.includes(key)) return
        if (value === null || value === undefined) return
        if (!defaultValidate(key, value, (defaults as Record<string, unknown>)[key])) return
        const cur = (config as Record<string, unknown>)[key]
        if (!deepEqual(cur, value)) {
          // biome-ignore lint: dynamic key assignment
          ;(config as Record<string, unknown>)[key] = value
        }
      }),
    )
    .catch(() => {})

  return config
}
