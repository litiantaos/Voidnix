import { reactive, watch } from 'vue'
import { load, type Store } from '@tauri-apps/plugin-store'

// store 实例缓存（§2.4 v1.5 B1）：避免每次防抖保存重新 load()。
// 缓存 Promise<Store> 而非 Store，并发 getStore 共享同一 in-flight load，避免重复加载。
const storeCache = new Map<string, Promise<Store>>()

function getStore(extId: string): Promise<Store> {
  let promise = storeCache.get(extId)
  if (!promise) {
    promise = load(`extensions/${extId}/config.json`)
    storeCache.set(extId, promise)
    // 失败时清缓存：避免单次 IO 抖动演变为永久故障（rejected Promise 被缓存）
    promise.catch(() => storeCache.delete(extId))
  }
  return promise
}

/// 原始类型用 ===；引用类型（对象/数组）走 JSON 序列化深度相等比较。
/// 用于 race 保护：判断磁盘 load 完成时当前值是否仍是 default（用户尚未触碰此 key）。
function isStillDefault(cur: unknown, def: unknown): boolean {
  if (Object.is(cur, def)) return true
  // 引用型：结构相同视为 default（用户尚未结构变更）
  if (typeof cur !== typeof def) return false
  if (cur === null || def === null) return cur === def
  if (typeof cur !== 'object') return false
  try {
    return JSON.stringify(cur) === JSON.stringify(def)
  } catch {
    return false
  }
}

/// 创建响应式扩展配置对象，自动从磁盘加载 + 变更自动持久化。
///
/// 用法：
/// ```ts
/// const config = defineConfig('clipboard', { maxDays: 30, enabled: true })
/// config.maxDays     // → 30（响应式读取）
/// config.maxDays = 60  // 自动持久化到 extensions/clipboard/config.json
/// ```
///
/// 加载语义（v1.6 N10）：load() 异步，扩展 setup / 早期命令可能读到 defaults
/// （磁盘值尚未回填）。安全参数由 Rust clamp 兜底，UI 可能短暂显示 defaults。
///
/// 深克隆（C7）：defaults 用 structuredClone 深拷贝，避免嵌套对象/数组引用共享——
/// 否则 config.xxx.push(...) 会污染 defaults.xxx，破坏 race 保护与后续 defineConfig 纯净度。
export function defineConfig<T extends Record<string, unknown>>(extId: string, defaults: T): T {
  const config = reactive(structuredClone(defaults)) as T

  // 异步从磁盘加载已保存的值
  // 竞态保护：backfill 的 store.get 是异步的，返回前用户可能已改某 key。
  // 写入前检查"当前值是否仍为 default"——若已非 default 说明用户已改，跳过覆盖。
  getStore(extId)
    .then((store) =>
      Promise.all(
        Object.keys(defaults).map(async (key) => {
          const saved = await store.get<unknown>(key)
          if (saved !== null && saved !== undefined) {
            const cur = (config as Record<string, unknown>)[key]
            const def = (defaults as Record<string, unknown>)[key]
            // 仅当当前值仍为 default（用户尚未触碰此 key）才回填磁盘值
            // 引用型走深度相等（C7）：避免 `cur === def` 对引用永远 true 失效保护
            if (isStillDefault(cur, def)) {
              // biome-ignore lint: dynamic key assignment
              ;(config as Record<string, unknown>)[key] = saved
            }
          }
        }),
      ),
    )
    .catch((e) => console.error(`[config:${extId}] load failed:`, e))

  // 变更自动持久化（deep watch + 防抖 300ms）
  let saveTimer: ReturnType<typeof setTimeout> | null = null
  watch(
    config,
    () => {
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(async () => {
        try {
          const store = await getStore(extId)
          for (const key in defaults) {
            await store.set(key, config[key])
          }
          await store.save()
        } catch (e) {
          console.error(`[config:${extId}] save failed:`, e)
        }
      }, 300)
    },
    { deep: true },
  )

  return config
}
