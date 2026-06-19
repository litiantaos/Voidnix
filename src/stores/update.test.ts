import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useUpdateStore } from './update'

describe('update store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('初始状态', () => {
    const store = useUpdateStore()
    expect(store.downloading).toBe(false)
    expect(store.downloaded).toBe(false)
    expect(store.checking).toBe(false)
    expect(store.error).toBeNull()
    expect(store.info).toBeNull()
    expect(store.dialogVisible).toBe(false)
  })

  it('showDialog / closeDialog 切换对话框', () => {
    const store = useUpdateStore()
    store.showDialog()
    expect(store.dialogVisible).toBe(true)
    store.closeDialog()
    expect(store.dialogVisible).toBe(false)
  })

  it('reset 恢复全部初始状态', () => {
    const store = useUpdateStore()
    store.downloading = true
    store.info = { currentVersion: '1.0', newVersion: '2.0', body: null }
    store.dialogVisible = true
    store.error = 'some error'

    store.reset()

    expect(store.downloading).toBe(false)
    expect(store.downloaded).toBe(false)
    expect(store.checking).toBe(false)
    expect(store.error).toBeNull()
    expect(store.info).toBeNull()
    expect(store.dialogVisible).toBe(false)
  })

  it('非 Tauri 环境 check 返回 false', async () => {
    const store = useUpdateStore()
    const result = await store.check()
    expect(result).toBe(false)
  })
})
