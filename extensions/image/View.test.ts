import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

// 处理态任务行回归（见 docs/design.md「处理态」）：两个工具共用操作行——
// 处理中整行变不定进度条 + 阶段文案（图片链路无分母，不伪造百分比），
// 完成闪 600ms 后换成结果操作按钮；拼接同样有进度反馈（旧实现零反馈）。

interface PendingRun {
  onProgress: { onmessage: (msg: string) => void }
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

import View from './View.vue'
import { pendingInputPaths } from './index'

const RESULT = {
  previewDataUrl: 'data:image/png;base64,iVBORw0KGgo=',
  tempPath: '/tmp/voidnix_image_1.png',
  width: 10,
  height: 10,
  sizeBytes: 100,
}

beforeEach(() => {
  setActivePinia(createPinia())
  runs.length = 0
  invokeMock.mockReset()
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'image_read_preview':
        return Promise.resolve('data:image/png;base64,iVBORw0KGgo=')
      case 'image_remove_bg':
      case 'image_stitch':
        return new Promise((resolve, reject) => {
          runs.push({
            onProgress: (args.onProgress ?? args.on_progress) as PendingRun['onProgress'],
            resolve,
            reject,
          })
        })
      default:
        return Promise.resolve(undefined)
    }
  })
  pendingInputPaths.value = []
})

async function mountWithImage() {
  const wrapper = mount(View, { attachTo: document.body })
  pendingInputPaths.value = ['/tmp/photo.png']
  await flushPromises()
  return wrapper
}

describe('image 处理态任务行', () => {
  it('移除背景：处理中 = 标题 + 不定进度条 + 阶段文案，完成后换结果按钮', async () => {
    const wrapper = await mountWithImage()
    expect(wrapper.find('#si-operations button').text()).toBe('移除背景')

    await wrapper.find('#si-operations button').trigger('click')
    await flushPromises()
    let ops = wrapper.find('#si-operations')
    expect(ops.text()).toContain('正在移除背景')
    expect(ops.text()).toContain('正在抠图…')
    expect(ops.find('[role="progressbar"]').attributes('aria-valuenow')).toBeUndefined()
    // 图片链路不可取消：任务行无任何按钮
    expect(ops.find('button').exists()).toBe(false)

    runs[0].onProgress.onmessage('uploading')
    await flushPromises()
    expect(wrapper.find('#si-operations').text()).toContain('上传并处理中…')

    runs[0].resolve(RESULT)
    await flushPromises()
    ops = wrapper.find('#si-operations')
    expect(ops.text()).toContain('处理完成')
    expect(ops.find('[role="progressbar"]').attributes('aria-valuenow')).toBe('100')

    await new Promise((r) => setTimeout(r, 700))
    await flushPromises()
    const labels = wrapper.findAll('#si-operations button').map((b) => b.text())
    expect(labels).toEqual(['复制', '保存'])
    wrapper.unmount()
  })

  it('失败：任务行复位空闲态 + toast', async () => {
    const wrapper = await mountWithImage()
    await wrapper.find('#si-operations button').trigger('click')
    await flushPromises()
    runs[0].reject('模型失败')
    await flushPromises()
    expect(wrapper.find('#si-operations').text()).toContain('操作')
    expect(wrapper.find('#si-operations button').text()).toBe('移除背景')
    wrapper.unmount()
  })

  it('拼接：惰性生成同样有任务行进度反馈（旧实现零反馈）', async () => {
    const wrapper = mount(View, { attachTo: document.body })
    pendingInputPaths.value = ['/tmp/a.png', '/tmp/b.png']
    await flushPromises()
    // 拼接模式操作行 = 复制 + 保存（生成在点击时触发）
    expect(wrapper.findAll('#si-operations button').map((b) => b.text())).toEqual(['复制', '保存'])

    await wrapper.findAll('#si-operations button')[1].trigger('click')
    await flushPromises()
    expect(wrapper.find('#si-operations').text()).toContain('正在拼接长图')
    expect(wrapper.find('#si-operations').text()).toContain('正在拼接…')
    expect(wrapper.find('#si-operations [role="progressbar"]').exists()).toBe(true)

    runs[0].resolve(RESULT)
    await flushPromises()
    expect(wrapper.find('#si-operations').text()).toContain('处理完成')
    wrapper.unmount()
  })
})
