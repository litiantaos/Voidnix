import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

// 粘贴置顶即时刷新回归：粘贴（回车执行）成功后，Rust 端 hide_main 直接隐藏窗口、
// 不经前端 hideWindow（无 window-hiding 事件，onWindowHiding 的重拉不会触发），
// View 必须自行清缓存并重拉——refresh_pasted_at 刷新的置顶序与新时间才会落进
// history。此前只 invalidateCache + reset 不重拉，列表保持旧序旧时间，直到退出
// 扩展再进入（onActivated 重拉）才更新。

const OLD_ITEM = {
  id: '1000',
  content: '旧记录',
  content_type: 'text',
  source_app: 'Zed',
  created_at: '2026-09-26 08:00:00',
  is_favorite: false,
  score: 0,
  file_size: null,
  image_width: null,
  image_height: null,
}
const PASTED_ITEM = {
  id: '2000',
  content: '被粘贴记录',
  content_type: 'text',
  source_app: 'Safari',
  created_at: '2026-09-26 07:00:00',
  is_favorite: false,
  score: 0,
  file_size: null,
  image_width: null,
  image_height: null,
}
const PASTED_AT = '2026-09-26 09:00:00'

// 默认实现兜底：clipboard config.ts 顶层 watch immediate 在模块加载期即调 invoke，
// 须返回 Promise（mockImplementation 在 beforeEach 设置，晚于模块加载）
const { invokeMock } = vi.hoisted(() => {
  // isTauri 置真（模块加载前，utils/tauri 的 isTauri 常量按 __TAURI_INTERNALS__
  // 判定）：回退复制分支的 toast 走 show_toast invoke，需穿透 useToast 的环境守卫
  ;(globalThis as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
  return { invokeMock: vi.fn().mockImplementation(() => Promise.resolve(null)) }
})

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    // defineConfig 退出 flush 钩子（isTauri 置真后进入）：返回 unlisten 桩
    onCloseRequested: () => Promise.resolve(() => {}),
  }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))

import View from './View.vue'
import BaseList from '@/components/ui/BaseList.vue'
import { CMD } from '@/commands'
import { history, fetchClipboardHistory } from './index'
// 注册框架级文案（main.ts 职责，测试环境补位）：toast 断言按已解析文案匹配
import '@/locales'

function mockHistoryResponses() {
  // Rust 端 refresh_pasted_at 在 paste 命令内同步完成：粘贴后拉取即返回置顶新序
  let pasted = false
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === CMD.getClipboardHistory) {
      return Promise.resolve(
        pasted ? [{ ...PASTED_ITEM, created_at: PASTED_AT }, OLD_ITEM] : [OLD_ITEM, PASTED_ITEM],
      )
    }
    if (cmd === CMD.pasteClipboardItem || cmd === CMD.pasteClipboardItems) {
      pasted = true
      return Promise.resolve()
    }
    return Promise.resolve(null)
  })
}

function historyCalls(): number {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === CMD.getClipboardHistory).length
}

describe('clipboard View 粘贴置顶刷新', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    invokeMock.mockReset()
    mockHistoryResponses()
  })
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('粘贴成功后重拉列表：被贴记录置顶 + 时间刷新即时落进 history（不等退出扩展重进）', async () => {
    await fetchClipboardHistory('', false)
    expect(history.value.map((i) => i.id)).toEqual(['1000', '2000'])

    const wrapper = mount(View, { attachTo: document.body })
    await flushPromises()

    wrapper.findComponent(BaseList).vm.$emit('execute', history.value[1], 1)
    await flushPromises()

    // 粘贴命令已执行，且触发了第二次 get_clipboard_history（重拉，非缓存命中）
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === CMD.pasteClipboardItem)).toBe(true)
    expect(historyCalls()).toBe(2)

    // 新序：被贴记录置顶；时间已刷新为粘贴时间（由重拉数据承载）
    expect(history.value.map((i) => i.id)).toEqual(['2000', '1000'])
    expect(history.value[0].created_at).toBe(PASTED_AT)

    wrapper.unmount()
  })

  it('目标无输入区回退复制：命令返回 copied 时 toast「已复制」（置顶重拉照常）', async () => {
    await fetchClipboardHistory('', false)

    let used = false
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === CMD.getClipboardHistory) {
        return Promise.resolve(
          used ? [{ ...PASTED_ITEM, created_at: PASTED_AT }, OLD_ITEM] : [OLD_ITEM, PASTED_ITEM],
        )
      }
      if (cmd === CMD.pasteClipboardItem || cmd === CMD.pasteClipboardItems) {
        used = true
        return Promise.resolve('copied')
      }
      return Promise.resolve(null)
    })

    const wrapper = mount(View, { attachTo: document.body })
    await flushPromises()

    wrapper.findComponent(BaseList).vm.$emit('execute', history.value[1], 1)
    await flushPromises()

    // toast 按命令结果分流：回退复制 → 「已复制」
    const toastCall = invokeMock.mock.calls.find(([cmd]) => cmd === CMD.showToast)
    expect(toastCall?.[1]).toMatchObject({ message: '已复制' })
    // 复制同样刷新 created_at（等效最近使用）：置顶重拉照常
    expect(history.value.map((i) => i.id)).toEqual(['2000', '1000'])

    wrapper.unmount()
  })
})
