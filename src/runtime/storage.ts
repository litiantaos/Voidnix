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
  }
  return promise
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
export function defineConfig<T extends Record<string, unknown>>(extId: string, defaults: T): T {
  const config = reactive({ ...defaults }) as T

  // 异步从磁盘加载已保存的值
  getStore(extId)
    .then((store) =>
      Promise.all(
        Object.keys(defaults).map(async (key) => {
          const saved = await store.get<unknown>(key)
          if (saved !== null && saved !== undefined) {
            // biome-ignore lint: dynamic key assignment
            ;(config as Record<string, unknown>)[key] = saved
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
