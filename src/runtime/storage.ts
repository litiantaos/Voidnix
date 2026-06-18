import { reactive, watch } from 'vue'
import { load } from '@tauri-apps/plugin-store'

/// 扩展配置 schema 字段定义（供 settings 面板自动渲染）。
export interface ConfigField {
  type: 'number' | 'toggle' | 'string' | 'select' | 'text' | 'keybind'
  default: unknown
  label: string
  description?: string
  min?: number
  max?: number
  step?: number
  options?: Array<{ value: string; label: string }>
  order?: number
}

export type ConfigSchema = Record<string, ConfigField>

/// 声明扩展配置 schema（供 settings 面板自动渲染 + 类型推导）。
export function defineExtensionConfig<T extends ConfigSchema>(schema: T): T {
  return schema
}

/// 创建响应式扩展配置对象，自动从磁盘加载 + 变更自动持久化。
///
/// 用法：
/// ```ts
/// const config = defineConfig('clipboard', { maxDays: 30, enabled: true })
/// config.maxDays     // → 30（响应式读取）
/// config.maxDays = 60  // 自动持久化到 extensions/clipboard/config.json
/// ```
export function defineConfig<T extends Record<string, unknown>>(extId: string, defaults: T): T {
  const config = reactive({ ...defaults }) as T

  // 异步从磁盘加载已保存的值
  load(`extensions/${extId}/config.json`)
    .then(async (store) => {
      for (const key in defaults) {
        const saved = await store.get<unknown>(key)
        if (saved !== null && saved !== undefined) {
          // biome-ignore lint: dynamic key assignment
          ;(config as Record<string, unknown>)[key] = saved
        }
      }
    })
    .catch((e) => console.error(`[config:${extId}] load failed:`, e))

  // 变更自动持久化（deep watch + 防抖）
  let saveTimer: ReturnType<typeof setTimeout> | null = null
  watch(
    config,
    () => {
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(async () => {
        try {
          const store = await load(`extensions/${extId}/config.json`)
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
