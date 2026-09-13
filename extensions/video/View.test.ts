import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

// 回归：终态双投递（Channel + 全局 video-job-event）只弹一次 toast。
// 复现时序：Channel 终态先到 → finishBatch 同步清 queueActive/busy → 迟到的
// 全局同名终态不得再走孤儿分支重复反馈；孤儿观察（busy 由快照/progress 置起）不受影响。

const { handlers, jobSnapshot } = vi.hoisted(() => ({
  handlers: {} as Record<string, (e: { payload: unknown }) => void>,
  jobSnapshot: { busy: false, lastOutput: null, lastError: null, lastPercent: 0 },
}))

vi.mock('@tauri-apps/api/core', () => {
  // Channel 仅承载 onmessage 回调（组件赋值后测试直接调用即 Channel 投递）
  class Channel<T = unknown> {
    onmessage: ((msg: T) => void) | null = null
  }
  return { invoke: vi.fn(), Channel }
})
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb
    return () => {}
  }),
}))

import { useAppStore } from '@/stores/app'

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  for (const k of Object.keys(handlers)) delete handlers[k]
  jobSnapshot.busy = false
  jobSnapshot.lastPercent = 0
  vi.resetModules()
})

/** 重置模块注册表后加载全新模块状态（st 模块级 reactive 跨测试隔离）。 */
async function setup() {
  const { invoke } = await import('@tauri-apps/api/core')
  const { default: View } = await import('./View.vue')
  const { pendingInputPaths } = await import('./index')
  const invokeMock = vi.mocked(invoke)
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'video_core_status':
        return { available: true, source: 'test', version: '7.1', downloading: false }
      case 'video_probe':
        return { durationSecs: 60, width: 1920, height: 1080, sizeBytes: 1 }
      case 'video_job_status':
        return jobSnapshot
      default:
        return undefined
    }
  })
  const wrapper = mount(View, { attachTo: document.body })
  await flushPromises()
  return { wrapper, invokeMock, pendingInputPaths }
}

/** 选文件并点开始，返回 video_run 捕获的 Channel（模拟 Rust 侧事件源）。 */
async function startRun(ctx: Awaited<ReturnType<typeof setup>>): Promise<(ev: unknown) => void> {
  ctx.pendingInputPaths.value = ['/tmp/a.mp4']
  await flushPromises()
  await ctx.wrapper.find('#si-operations button').trigger('click')
  const call = ctx.invokeMock.mock.calls.find(([c]) => c === 'video_run')
  const channel = (call![1] as { onEvent: { onmessage: (ev: unknown) => void } }).onEvent
  return (ev) => channel.onmessage(ev)
}

describe('video 终态双投递去重', () => {
  it('取消：Channel 终态收束后，迟到的全局同名终态不再弹 toast', async () => {
    const ctx = await setup()
    const emitChannel = await startRun(ctx)
    const appStore = useAppStore()
    const spy = vi.spyOn(appStore, 'showStatus')

    const ev = { type: 'error', message: '已取消' }
    emitChannel(ev)
    handlers['video-job-event']({ payload: ev })

    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy).toHaveBeenCalledWith('已取消')
    ctx.wrapper.unmount()
  })

  it('完成：done 终态双投递只弹一次', async () => {
    const ctx = await setup()
    const emitChannel = await startRun(ctx)
    const appStore = useAppStore()
    const spy = vi.spyOn(appStore, 'showStatus')

    const ev = { type: 'done', outputPath: '/tmp/out.mp4', sizeBytes: 1 }
    emitChannel(ev)
    handlers['video-job-event']({ payload: ev })

    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy).toHaveBeenCalledWith('处理完成')
    ctx.wrapper.unmount()
  })

  it('孤儿观察不受影响：busy 由快照置起，全局事件照常驱动并反馈一次', async () => {
    jobSnapshot.busy = true
    jobSnapshot.lastPercent = 10
    const ctx = await setup()
    const appStore = useAppStore()
    const spy = vi.spyOn(appStore, 'showStatus')

    // 回归：孤儿态 paths 为空但 busy 置起，「操作」行仍渲染取消入口（Start 恒禁用）
    const opsButtons = ctx.wrapper.findAll('#si-operations button')
    expect(opsButtons).toHaveLength(1)
    expect(opsButtons[0].text()).toBe('取消')

    handlers['video-job-event']({
      payload: { type: 'progress', percent: 50, timeSecs: 30, speed: '2x' },
    })
    handlers['video-job-event']({ payload: { type: 'error', message: 'boom' } })

    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy).toHaveBeenCalledWith('失败：boom', expect.anything())
    ctx.wrapper.unmount()
  })
})
