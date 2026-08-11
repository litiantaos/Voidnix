import { ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauri } from '@/utils/tauri'
import { useSettingsStore } from '@/stores/settings'

// ─── 类型 ────────────────────────────────────────────────────

export type Locale = 'zh-CN' | 'en'

/** 本地化文本：纯字符串（全语言通用，向后兼容）或按语言区分的映射。 */
export type LocalizedText = string | Partial<Record<Locale, string>>

// ─── 状态 ────────────────────────────────────────────────────

const STORAGE_KEY = 'voidnix-locale'

/** 当前语言（模块级响应式单一源）。t() / resolveLocalized() 均读 .value 建立响应式依赖。 */
export const locale = ref<Locale>('zh-CN')

/** 文案表：key → 各语言文案。registerMessages 合并写入。 */
const messageMap: Record<string, Partial<Record<Locale, string>>> = {}

let initialized = false

// ─── 核心 API ────────────────────────────────────────────────

/** 注册文案（合并）。框架启动 + 扩展 import side-effect 调用，与 defineExtension 同范式。 */
export function registerMessages(msgs: Record<string, Partial<Record<Locale, string>>>): void {
  for (const [key, val] of Object.entries(msgs)) {
    const existing = messageMap[key]
    if (existing) {
      Object.assign(existing, val)
    } else {
      messageMap[key] = { ...val }
    }
  }
}

/** 翻译：读 locale.value 查表，回退 zh-CN，再回退 key 本身。{param} 占位符插值。 */
export function t(key: string, params?: Record<string, string | number>): string {
  const entry = messageMap[key]
  const raw =
    entry?.[locale.value] ?? entry?.['zh-CN'] ?? entry?.en ?? Object.values(entry ?? {})[0] ?? key
  if (!params) return raw
  return raw.replace(/\{(\w+)\}/g, (_, k: string) => String(params[k] ?? `{${k}}`))
}

/** 解析 LocalizedText：按当前语言取值，回退 zh-CN → 首个可用值 → 原值。 */
export function resolveLocalized(text: LocalizedText | undefined): string {
  if (text == null) return ''
  if (typeof text === 'string') return text
  return text[locale.value] ?? text['zh-CN'] ?? text.en ?? Object.values(text)[0] ?? ''
}

// ─── 初始化 ──────────────────────────────────────────────────

/** 初始化 i18n：main.ts pinia 就绪后调用（与 initTheme 同范式）。
 *  main：settings.language 驱动 + 写 localStorage 供子窗口同步。
 *  子窗口：读 localStorage + 监听 storage 事件实时响应切换。 */
export function initI18n(): void {
  if (initialized) return
  initialized = true

  if (!isTauri || getCurrentWindow().label === 'main') {
    const settings = useSettingsStore()
    // clamping：磁盘 config 可能被手改为无效值，归一到合法 Locale
    const raw = settings.language
    const valid: Locale = raw === 'en' ? 'en' : 'zh-CN'
    if (raw !== valid) settings.language = valid
    locale.value = valid
    watch(
      () => settings.language,
      (l) => {
        const v: Locale = l === 'en' ? 'en' : 'zh-CN'
        locale.value = v
        if (l !== v) settings.language = v
        try {
          localStorage.setItem(STORAGE_KEY, v)
        } catch {
          // ignore
        }
      },
    )
    try {
      localStorage.setItem(STORAGE_KEY, valid)
    } catch {
      // ignore
    }
    return
  }

  // 子窗口：localStorage 同步 + storage 事件实时响应
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'zh-CN' || stored === 'en') locale.value = stored
  } catch {
    // ignore
  }
  window.addEventListener('storage', (e) => {
    if (e.key === STORAGE_KEY && (e.newValue === 'zh-CN' || e.newValue === 'en')) {
      locale.value = e.newValue
    }
  })
}
