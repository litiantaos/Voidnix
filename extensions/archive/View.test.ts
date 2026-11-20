import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

// 处理态任务行回归：操作行空闲 = 主按钮，处理中整行变进度条 + 取消，完成闪 600ms，
// 失败统一 toast 且不落常驻错误行（见 docs/design.md「处理态」）。

interface PendingRun {
  onEvent: { onmessage: (ev: unknown) => void }
  resolve: (v: unknown) => void
  reject: (e: unknown) => void
}

const { runs, invokeMock } = vi.hoisted(() => ({
  runs: [] as PendingRun[],
  invokeMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => {
  class Channel<T = unknown> {
    onmessage: ((msg: T) => void) | null = null
  }
  return { invoke: invokeMock, Channel }
})

import { useAppStore } from '@/stores/app'
import View from './View.vue'
import { pendingInputPaths } from './index'

/** 记录 archive_compress / archive_extract 的 Channel 与 deferred（测试驱动终态）。 */
function pendingInvoke(args: { onEvent: PendingRun['onEvent'] }) {
  return new Promise((resolve, reject) => {
    runs.push({ onEvent: args.onEvent, resolve, reject })
  })
}

beforeEach(() => {
  setActivePinia(createPinia())
  runs.length = 0
  invokeMock.mockReset()
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'archive_inspect':
        return Promise.resolve({
          path: '/tmp/pkg.zip',
          format: 'zip',
          entries: [{ path: 'a.txt', kind: 'file', size: 10 }],
          totalSize: 10,
          encrypted: false,
          truncated: false,
        })
      case 'archive_compress':
      case 'archive_extract':
        return pendingInvoke(args as { onEvent: PendingRun['onEvent'] })
      default:
        return Promise.resolve(undefined)
    }
  })
  pendingInputPaths.value = []
})

async function mountCompress() {
  const wrapper = mount(View, { attachTo: document.body })
  pendingInputPaths.value = ['/tmp/report.txt']
  await flushPromises()
  return wrapper
}

describe('archive 处理态任务行', () => {
  it('空闲 = 操作行主按钮；处理中 = 标题 + 进度条 + 取消；源行不再承载按钮', async () => {
    const wrapper = await mountCompress()
    expect(wrapper.find('#si-operations').text()).toContain('操作')
    expect(wrapper.find('#si-operations button').text()).toBe('压缩')
    // 源行只剩信息，按钮已上移到操作行
    expect(wrapper.find('#si-source button').exists()).toBe(false)

    await wrapper.find('#si-operations button').trigger('click')
    await flushPromises()
    const ops = wrapper.find('#si-operations')
    expect(ops.text()).toContain('正在压缩')
    // 首个 progress 事件前无分母：不定进度条（无 aria-valuenow）+ 取消
    expect(ops.find('[role="progressbar"]').attributes('aria-valuenow')).toBeUndefined()
    expect(ops.find('button').text()).toBe('取消')

    runs[0].onEvent.onmessage({ type: 'progress', percent: 45 })
    await flushPromises()
    expect(wrapper.find('#si-operations [role="progressbar"]').attributes('aria-valuenow')).toBe(
      '45',
    )
    wrapper.unmount()
  })

  it('失败：任务行复位空闲态 + toast，不新增常驻错误行', async () => {
    const wrapper = await mountCompress()
    const appStore = useAppStore()
    const spy = vi.spyOn(appStore, 'showStatus')

    await wrapper.find('#si-operations button').trigger('click')
    await flushPromises()
    runs[0].onEvent.onmessage({ type: 'error', message: '磁盘已满' })
    await flushPromises()

    expect(wrapper.find('#si-operations').text()).toContain('操作')
    expect(wrapper.find('#si-error').exists()).toBe(false)
    expect(spy).toHaveBeenCalledWith('磁盘已满', expect.anything())

    // invoke reject 同报同一错误（双通道）：按消息去重，不二次 toast
    runs[0].reject('磁盘已满')
    await flushPromises()
    expect(spy).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('成功：满进度 + 完成文案停留 600ms 后揭晓目标并关窗，再复位', async () => {
    const wrapper = await mountCompress()
    await wrapper.find('#si-operations button').trigger('click')
    await flushPromises()
    runs[0].onEvent.onmessage({ type: 'progress', percent: 90 })
    runs[0].resolve({ outputPath: '/tmp/report.zip', fileCount: 3, skippedJunk: 0, sizeBytes: 42 })
    await flushPromises()

    const ops = wrapper.find('#si-operations')
    expect(ops.text()).toContain('压缩完成')
    expect(ops.find('[role="progressbar"]').attributes('aria-valuenow')).toBe('100')
    expect(ops.find('button').exists()).toBe(false)

    await new Promise((r) => setTimeout(r, 700))
    await flushPromises()
    expect(invokeMock.mock.calls.some(([c]) => c === 'reveal_in_finder')).toBe(true)
    expect(invokeMock.mock.calls.some(([c]) => c === 'hide_window')).toBe(true)
    expect(wrapper.find('#si-operations').text()).toContain('操作')
    wrapper.unmount()
  })

  it('解压模式同样走操作行任务条', async () => {
    const wrapper = mount(View, { attachTo: document.body })
    pendingInputPaths.value = ['/tmp/pkg.zip']
    await flushPromises()
    expect(wrapper.find('#si-operations button').text()).toBe('解压')
    expect(wrapper.find('#si-archive button').exists()).toBe(false)

    await wrapper.find('#si-operations button').trigger('click')
    await flushPromises()
    expect(wrapper.find('#si-operations').text()).toContain('正在解压')
    expect(wrapper.find('#si-operations button').text()).toBe('取消')
    wrapper.unmount()
  })
})
