import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore } from './settings'

// 模拟 plugin-store：内存表（get/set/save/clear/onChange）
const memStore = new Map<string, unknown>()
const storeGet = vi.fn((k: string) => Promise.resolve(memStore.get(k)))
const storeSet = vi.fn((k: string, v: unknown) => {
  memStore.set(k, v)
  return Promise.resolve()
})
const storeSave = vi.fn(() => Promise.resolve())
const storeClear = vi.fn(() => {
  memStore.clear()
  return Promise.resolve()
})
const storeOnChange = vi.fn(() => Promise.resolve(() => {}))

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: storeGet,
      set: storeSet,
      save: storeSave,
      clear: storeClear,
      onChange: storeOnChange,
    }),
}))

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

describe('settings store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    memStore.clear()
    storeGet.mockClear()
    storeSet.mockClear()
    storeSave.mockClear()
    storeClear.mockClear()
  })

  it('初始默认值', () => {
    const store = useSettingsStore()
    expect(store.globalShortcut).toBe('Alt+Space')
  })

  describe('shortcutOverride', () => {
    it('getShortcutOverride 返回已设置的值', () => {
      const store = useSettingsStore()
      expect(store.getShortcutOverride('translate')).toBeUndefined()
    })

    it('setShortcutOverride 后能读取', async () => {
      const store = useSettingsStore()
      await store.setShortcutOverride('translate', 'CommandOrControl+T')
      expect(store.getShortcutOverride('translate')).toBe('CommandOrControl+T')
    })
  })
})
