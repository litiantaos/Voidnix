import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(),
}))

import UpdateDialog from './UpdateDialog.vue'
import { useUpdateStore } from '@/stores/update'
import '@/locales'

const mountedWrappers: ReturnType<typeof mount>[] = []

function mountDialog() {
  const wrapper = mount(UpdateDialog, { attachTo: document.body })
  mountedWrappers.push(wrapper)
  return wrapper
}

// BaseDialog Teleport 到 body，文本与按钮直接查 document.body
const bodyText = () => document.body.textContent ?? ''
const buttonByText = (text: string) =>
  Array.from(document.querySelectorAll('button')).find((b) => b.textContent === text)

describe('UpdateDialog 全流程形态', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  it('检查中：文案 + 不定进度条（复用下载进度轨道），单按钮「取消」（无稍后）', () => {
    const store = useUpdateStore()
    store.checking = true

    mountDialog()

    expect(bodyText()).toContain('正在检查更新…')
    expect(
      document.querySelector('.progress-track .progress-fill.progress-indeterminate'),
    ).toBeTruthy()
    expect(buttonByText('取消')).toBeTruthy()
    expect(buttonByText('稍后')).toBeFalsy()
  })

  it('无更新：已是最新版本文案 + 单按钮「好的」', () => {
    const store = useUpdateStore()
    store.currentVersion = '1.2.3'

    mountDialog()

    expect(bodyText()).toContain('已是最新版本')
    expect(bodyText()).toContain('1.2.3')
    expect(buttonByText('好的')).toBeTruthy()
    expect(buttonByText('稍后')).toBeFalsy()
  })

  it('失败：错误信息 + 「重试 / 取消」双按钮，点重试走 startCheck 且不关窗', async () => {
    const store = useUpdateStore()
    store.error = 'boom'
    const startCheckSpy = vi.spyOn(store, 'startCheck')

    mountDialog()

    expect(bodyText()).toContain('boom')
    expect(buttonByText('取消')).toBeTruthy()

    const retry = buttonByText('重试')
    expect(retry).toBeTruthy()
    retry!.click()
    await nextTick()

    expect(startCheckSpy).toHaveBeenCalledOnce()
    // 重试不关窗（closeOnConfirm=false，弹窗保持承载新一轮检查）
    expect(store.dialogVisible).toBe(true)
  })

  it('发现新版本：版本对比 + 「下载并安装 / 稍后」双按钮', () => {
    const store = useUpdateStore()
    store.info = { currentVersion: '1.0.0', newVersion: '2.0.0', body: null }

    mountDialog()

    expect(bodyText()).toContain('发现新版本')
    expect(bodyText()).toContain('v1.0.0')
    expect(bodyText()).toContain('v2.0.0')
    expect(buttonByText('下载并安装')).toBeTruthy()
    expect(buttonByText('稍后')).toBeTruthy()
  })
})
