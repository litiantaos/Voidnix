import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { flushPromises } from '@vue/test-utils'

// 走真实 check() 路径：mock 环境判定与 updater 插件（setup store 的 action 间为闭包调用，
// spyOn store 属性拦不到内部调用，断言一律落在 store 状态上）
vi.mock('@/utils/tauri', () => ({ isTauri: true }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn().mockResolvedValue('9.9.9') }))
const { checkUpdate, invoke } = vi.hoisted(() => ({ checkUpdate: vi.fn(), invoke: vi.fn() }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: checkUpdate }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

import { useUpdateStore } from './update'

describe('update store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    checkUpdate.mockReset()
    invoke.mockReset().mockResolvedValue(undefined)
  })

  it('初始状态', () => {
    const store = useUpdateStore()
    expect(store.downloading).toBe(false)
    expect(store.downloaded).toBe(false)
    expect(store.checking).toBe(false)
    expect(store.error).toBeNull()
    expect(store.info).toBeNull()
    expect(store.progress).toBe(0)
    expect(store.dialogVisible).toBe(false)
    expect(store.currentVersion).toBe('')
  })

  it('closeDialog 关闭对话框', () => {
    const store = useUpdateStore()
    store.dialogVisible = true
    store.closeDialog()
    expect(store.dialogVisible).toBe(false)
  })

  it('reset 恢复全部初始状态', () => {
    const store = useUpdateStore()
    store.downloading = true
    store.info = { currentVersion: '1.0', newVersion: '2.0', body: null }
    store.dialogVisible = true
    store.progress = 0.5
    store.error = 'some error'
    store.currentVersion = '1.0'

    store.reset()

    expect(store.downloading).toBe(false)
    expect(store.downloaded).toBe(false)
    expect(store.checking).toBe(false)
    expect(store.error).toBeNull()
    expect(store.info).toBeNull()
    expect(store.progress).toBe(0)
    expect(store.dialogVisible).toBe(false)
    expect(store.currentVersion).toBe('')
    // 菜单栏「检查更新」项文案还原
    expect(invoke).toHaveBeenCalledWith('set_update_version', { version: null })
  })

  it('check 发现更新：填 info + currentVersion，返回 true', async () => {
    checkUpdate.mockResolvedValue({ available: true, version: '2.0.0', body: 'fixes' })
    const store = useUpdateStore()

    const result = await store.check()

    expect(result).toBe(true)
    expect(store.info).toEqual({ currentVersion: '9.9.9', newVersion: '2.0.0', body: 'fixes' })
    expect(store.currentVersion).toBe('9.9.9')
    expect(store.checking).toBe(false)
    // 菜单栏项切换为「更新到新版本（2.0.0）」
    expect(invoke).toHaveBeenCalledWith('set_update_version', { version: '2.0.0' })
  })

  it('check 无更新：返回 false 不填 info；失败：error 置位', async () => {
    const store = useUpdateStore()

    checkUpdate.mockResolvedValue(null)
    expect(await store.check()).toBe(false)
    expect(store.info).toBeNull()
    expect(store.error).toBeNull()
    expect(invoke).not.toHaveBeenCalled()

    checkUpdate.mockRejectedValue(new Error('network down'))
    expect(await store.check()).toBe(false)
    expect(store.error).toContain('network down')
  })
})

describe('startCheck（主动检查统一入口：立即弹窗、弹窗内检查）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    checkUpdate.mockReset()
    invoke.mockReset().mockResolvedValue(undefined)
  })

  it('无已有结果时立即弹窗并发起检查，无更新则弹窗停留结果态', async () => {
    checkUpdate.mockResolvedValue(null)
    const store = useUpdateStore()

    store.startCheck()
    await flushPromises()

    expect(store.dialogVisible).toBe(true)
    expect(checkUpdate).toHaveBeenCalledOnce()
    expect(store.info).toBeNull()
    expect(store.error).toBeNull()
  })

  it('已有更新信息时仅重新弹窗，不重复检查', async () => {
    const store = useUpdateStore()
    store.info = { currentVersion: '9.9.9', newVersion: '2.0.0', body: null }

    store.startCheck()
    await flushPromises()

    expect(store.dialogVisible).toBe(true)
    expect(checkUpdate).not.toHaveBeenCalled()
  })

  it('检查进行中幂等：不重复发起，弹窗保持', async () => {
    const store = useUpdateStore()
    store.checking = true

    store.startCheck()
    await flushPromises()

    expect(store.dialogVisible).toBe(true)
    expect(checkUpdate).not.toHaveBeenCalled()
  })

  it('失败残留（弹窗已关）时清错误重新检查', async () => {
    checkUpdate.mockResolvedValue(null)
    const store = useUpdateStore()
    store.error = 'boom'
    store.dialogVisible = false

    store.startCheck()
    await flushPromises()

    expect(store.error).toBeNull()
    expect(store.dialogVisible).toBe(true)
    expect(checkUpdate).toHaveBeenCalledOnce()
  })
})
