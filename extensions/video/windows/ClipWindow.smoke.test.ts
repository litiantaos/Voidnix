import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'

// 剪辑窗口挂载冒烟：双素材 bootstrap → 操作栏倍速下拉 → 素材页签 + 源行跳转
// 不抛错。剪辑窗是独立入口（clip.html），主 View 测试覆盖不到；happy-dom canvas 为空
// 实现，绘制路径由真机验证，这里守住挂载与页签/浮层交互的渲染回归。

const meta = {
  path: '/tmp/a.mp4',
  durationUs: 60_000_000,
  width: 1920,
  height: 1080,
  displayWidth: 1920,
  displayHeight: 1080,
  fpsNum: 30,
  fpsDen: 1,
  videoCodec: 'h264',
  codecTag: 'avc1',
  pixFmt: 'yuv420p',
  rotation: 0,
  hasAudio: true,
  audioCodec: 'aac',
  sampleRate: 48000,
  channels: 2,
  sizeBytes: 1000,
  container: 'mp4',
  previewable: true,
}

vi.mock('@tauri-apps/api/core', () => {
  class Channel<T = unknown> {
    onmessage: ((msg: T) => void) | null = null
  }
  return {
    invoke: vi.fn(async (cmd: string) => {
      if (cmd === 'video_clip_probe') return [meta, { ...meta, path: '/tmp/b.mp4' }]
      if (cmd === 'video_clip_take_inputs') return ['/tmp/a.mp4', '/tmp/b.mp4']
      if (cmd === 'video_media_open')
        return {
          origin: '',
          items: [
            { path: '/tmp/a.mp4', url: 'http://127.0.0.1:1/a.mp4' },
            { path: '/tmp/b.mp4', url: 'http://127.0.0.1:1/b.mp4' },
          ],
        }
      if (cmd === 'video_clip_timeline')
        return {
          spritePath: '/tmp/s.png',
          frameWidth: 160,
          frameHeight: 90,
          count: 10,
          intervalUs: 1000,
        }
      return undefined
    }),
    Channel,
    convertFileSrc: (p: string) => p,
  }
})
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ setFocus: async () => {} }),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ startDragging: async () => {} }),
}))

beforeEach(() => vi.resetModules())

describe('ClipWindow 挂载冒烟', () => {
  it('双素材挂载：倍速 BaseSelect + 素材页签 + 源行跳转不抛错', async () => {
    const { default: ClipWindow } = await import('./ClipWindow.vue')
    const wrapper = mount(ClipWindow, { attachTo: document.body })
    await flushPromises()
    await new Promise((r) => setTimeout(r, 20))
    // 底部操作栏倍速下拉（BaseSelect 图标模式，行为与 settings 选择框一致）
    await wrapper.find('.panel-actions .custom-select').trigger('click')
    await flushPromises()
    expect(document.querySelector('.dropdown-panel')).toBeTruthy()
    // 素材页签 → 源行跳转（不切页签）
    await wrapper.findAll('aside button')[1].trigger('click')
    const rows = wrapper.findAll('.source-row')
    expect(rows.length).toBe(2)
    await rows.at(-1)!.trigger('click')
    expect(wrapper.findAll('.source-row').length).toBe(2)
    wrapper.unmount()
  })
})
