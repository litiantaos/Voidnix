import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h, ref, computed, nextTick, type Ref } from 'vue'

// —— 共享状态（vi.hoisted 与 vi.mock 同步提升，工厂闭包可安全引用）——
// 全 Cocoa 语义（y 向上、frame.y 为底边）：单屏 visibleFrame y∈[0, 982]（已扣菜单栏/Dock），
// frame 跟随 set_main_frame 更新（get_main_frame 读回），还原真实反馈回路。
const state = vi.hoisted(() => ({
  frame: { x: 18, y: 300, width: 720, height: 480 },
  rustVisible: true,
  invokes: [] as Array<{ cmd: string; args?: Record<string, unknown> }>,
}))

const VIS_BOTTOM = 0
const VIS_TOP = 982
const MAX_HEIGHT = 800

vi.mock('@/utils/tauri', () => ({ isTauri: true }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onFocusChanged: async (cb: (e: { payload: boolean }) => void) => {
      ;(state as { focusCb?: unknown }).focusCb = cb
      return () => {}
    },
    onMoved: async (cb: () => void) => {
      ;(state as { moveCb?: unknown }).moveCb = cb
      return () => {}
    },
  }),
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: Record<string, unknown>) => {
    state.invokes.push({ cmd, args })
    if (cmd === 'is_main_window_visible') return state.rustVisible
    if (cmd === 'get_main_frame') {
      return {
        x: state.frame.x,
        y: state.frame.y,
        width: state.frame.width,
        height: state.frame.height,
        visBottomY: VIS_BOTTOM,
        visTopY: VIS_TOP,
        maxHeight: MAX_HEIGHT,
      }
    }
    if (cmd === 'set_main_frame') {
      state.frame = {
        x: args!.x as number,
        y: args!.y as number,
        width: args!.width as number,
        height: args!.height as number,
      }
    }
    return undefined
  },
}))

import { useExtensionHeight } from './useExtensionHeight'
import { WINDOW } from '@/runtime/constants'
import type { Extension } from '@/runtime/types'

function makeExt(id: string, windowHeight: Extension['windowHeight']): Extension {
  return { meta: { id, name: id, icon: 'i', order: 1 }, windowHeight } as unknown as Extension
}

const AUTO_EXT = makeExt('auto-ext', 'auto')
const FIXED_EXT = makeExt('fixed-ext', 820)

function focusCb(): ((e: { payload: boolean }) => void) | null {
  return ((state as { focusCb?: unknown }).focusCb as typeof focusCb) ?? null
}

function moveCb(): (() => void) | null {
  return ((state as { moveCb?: unknown }).moveCb as typeof moveCb) ?? null
}

function makeHarness() {
  const activeExtension = ref<Extension | null>(null)
  const activeSubview = ref<string | null>(null)
  // auto 模式量内容高：stub offsetHeight（happy-dom 无布局引擎）
  let contentHeight = 600
  const contentEl = {
    get offsetHeight() {
      return contentHeight
    },
  } as HTMLElement
  const contentRef: Ref<HTMLElement | undefined> = ref(contentEl)

  const TestComp = defineComponent({
    setup() {
      useExtensionHeight({
        activeExtension: computed(() => activeExtension.value),
        activeSubview: computed(() => activeSubview.value),
        contentRef,
      })
      return () => h('div')
    },
  })
  const wrapper = mount(TestComp)

  return {
    wrapper,
    activeExtension,
    setContentHeight(h: number) {
      contentHeight = h
    },
    async settle() {
      await nextTick()
      await flushPromises()
      await nextTick()
      await flushPromises()
    },
  }
}

/** 等待并清空 invoke 记录，返回期间发生的 setMainFrame 调用（不含已清空记录） */
async function drainSetMainFrame(h: ReturnType<typeof makeHarness>) {
  await h.settle()
  const calls = state.invokes.filter((i) => i.cmd === 'set_main_frame')
  state.invokes.length = 0
  return calls
}

beforeEach(() => {
  state.frame = { x: 18, y: 300, width: 720, height: 480 }
  state.rustVisible = true
  ;(state as { focusCb?: unknown }).focusCb = null
  ;(state as { moveCb?: unknown }).moveCb = null
  state.invokes.length = 0
  // happy-dom 无 ResizeObserver：stub（auto 模式 syncObserver 需要）
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  )
  // rAF → 微任务：scheduleAdjust 的双帧等待在 flushPromises 的微任务清空中即可推进
  // （嵌套注册的第二帧微任务同轮清空），不依赖 happy-dom 的真实帧时钟
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    queueMicrotask(() => cb(performance.now()))
    return 0
  })
  vi.stubGlobal('cancelAnimationFrame', () => {})
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('useExtensionHeight', () => {
  it('窗口可见性假阴性时经 Rust 权威状态复核自愈，fixed 高度照常还原', async () => {
    // 无 focus 事件（windowVisible=false）+ 窗口实际停留 fixed 820（如 agent 遗留）
    state.frame = { x: 18, y: 100, width: 720, height: 820 }
    const h = makeHarness()
    // 挂载后 adjust：复核 is_main_window_visible=true → 继续 → default 480 + 保顶边
    const calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(1)
    expect(calls[0].args?.height).toBe(WINDOW.DEFAULT_HEIGHT)
    // 顶边锚定：顶边 100+820=920 保持，y = 920-480
    expect(calls[0].args?.y).toBe(920 - WINDOW.DEFAULT_HEIGHT)
    h.wrapper.unmount()
  })

  it('Rust 权威可见性为 false（真隐藏）时跳过 set_main_frame', async () => {
    state.rustVisible = false
    state.frame = { x: 18, y: 100, width: 720, height: 820 }
    const h = makeHarness()
    await h.settle()
    // 不经 drainSetMainFrame（会清空记录）：直接断言原始 invokes
    expect(state.invokes.filter((i) => i.cmd === 'set_main_frame').length).toBe(0)
    expect(state.invokes.some((i) => i.cmd === 'is_main_window_visible')).toBe(true)
    h.wrapper.unmount()
  })

  it('已在目标高度时不重复 invoke（跳过条件回填）', async () => {
    const h = makeHarness()
    const calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(0)
    h.wrapper.unmount()
  })

  it('auto 增高底边出屏上移顶边，离开 auto 还原进入前顶边', async () => {
    const h = makeHarness()
    await drainSetMainFrame(h) // 初始 default 480 已达标，清空记录

    // 进入 auto：内容撑到 800（CHROME 76 + 800 → clamp maxHeight 800）
    h.setContentHeight(800)
    h.activeExtension.value = AUTO_EXT
    let calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(1)
    expect(calls[0].args?.height).toBe(800)
    // 底边 780-800=-20 < visBottom 0+40 → 上移：顶边 = min(0+40+800, 982) = 840，y = 840-800
    expect(calls[0].args?.y).toBe(40)

    // 离开 auto 回 default：顶边还原进入前 780，y = 780-480
    h.activeExtension.value = null
    calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(1)
    expect(calls[0].args?.height).toBe(WINDOW.DEFAULT_HEIGHT)
    expect(calls[0].args?.y).toBe(780 - WINDOW.DEFAULT_HEIGHT)
    h.wrapper.unmount()
  })

  it('auto 内容不足 DEFAULT 时高度钳下限，顶边全程锚定', async () => {
    const h = makeHarness()
    await drainSetMainFrame(h)

    h.setContentHeight(300) // 76+300=376 < DEFAULT 480 → 钳到 480，底边 300 ≥ 40 不上移
    h.activeExtension.value = AUTO_EXT
    let calls = await drainSetMainFrame(h)
    expect(calls[0].args?.height).toBe(WINDOW.DEFAULT_HEIGHT)
    expect(calls[0].args?.y).toBe(780 - WINDOW.DEFAULT_HEIGHT)

    // 离开 auto：高度与顶边均未变化（未上移），目标即现状，不下发
    h.activeExtension.value = null
    calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(0)
    h.wrapper.unmount()
  })

  it('focus 重置逻辑坐标缓存并以实际位置为基准', async () => {
    const h = makeHarness()
    await drainSetMainFrame(h)

    // 窗口被外部移动（如 present 重定位）：focus 后丢弃缓存、以实际为准
    state.frame = { x: 100, y: 50, width: 720, height: 480 }
    focusCb()?.({ payload: true })
    h.activeExtension.value = FIXED_EXT
    const calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(1)
    expect(calls[0].args?.height).toBe(820)
    // 实际顶边 50+480=530 保持，x 跟随实际 100
    expect(calls[0].args?.y).toBe(530 - 820)
    expect(calls[0].args?.x).toBe(100)
    h.wrapper.unmount()
  })

  it('连续切换 fixed→default 每次都下发目标高度', async () => {
    // 顶边 880（y 400 + h 480）：fixed 820 底边 60 不出屏，几何合法
    state.frame = { x: 18, y: 400, width: 720, height: 480 }
    const h = makeHarness()
    await drainSetMainFrame(h)

    h.activeExtension.value = FIXED_EXT
    let calls = await drainSetMainFrame(h)
    expect(calls[0].args?.height).toBe(820)
    expect(calls[0].args?.y).toBe(880 - 820)

    h.activeExtension.value = null
    calls = await drainSetMainFrame(h)
    expect(calls[0].args?.height).toBe(WINDOW.DEFAULT_HEIGHT)
    expect(calls[0].args?.y).toBe(880 - WINDOW.DEFAULT_HEIGHT)
    h.wrapper.unmount()
  })

  it('用户拖动后切扩展，位置以拖动后实际位置为基准（小幅拖动不回弹）', async () => {
    // happy-dom 的 performance.now 距窗口创建仅数十 ms，须受控时钟保证超出抑制窗口
    const perfSpy = vi.spyOn(performance, 'now').mockImplementation(() => 10_000)
    const h = makeHarness()
    await drainSetMainFrame(h) // 初始 default 480 达标无 invoke，缓存锚定 (18, 780)

    // 拖动 <80px：实际 frame 变化（不经 set_main_frame），onMoved 到达且超出抑制窗口
    state.frame = { x: 40, y: 320, width: 720, height: 480 } // 顶边 800，Δx/Δtop 均 <80
    moveCb()?.()

    h.activeExtension.value = FIXED_EXT
    const calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(1)
    expect(calls[0].args?.x).toBe(40) // 拖动后 x，非缓存 18
    expect(calls[0].args?.height).toBe(820)
    expect(calls[0].args?.y).toBe(800 - 820) // 保拖动后顶边
    perfSpy.mockRestore()
    h.wrapper.unmount()
  })

  it('set_main_frame 后 400ms 内的移动事件视为动画中间态，不重锚', async () => {
    const now = 10_000
    const perfSpy = vi.spyOn(performance, 'now').mockImplementation(() => now)
    const h = makeHarness()
    await drainSetMainFrame(h) // 缓存锚定 (18, 780)

    h.activeExtension.value = FIXED_EXT
    await drainSetMainFrame(h) // invoke set_main_frame，lastFrameSetAt = 10000

    // 同刻到达的移动事件（动画中间态，距逻辑目标 <80px）：缓存不失效
    state.frame = { x: 40, y: -60, width: 720, height: 820 } // 顶边 760，Δx/Δtop 均 <80
    moveCb()?.()

    h.activeExtension.value = null
    const calls = await drainSetMainFrame(h)
    expect(calls.length).toBe(1)
    // 基准取逻辑目标 x=18（守卫命中缓存），不取动画中间态 40
    expect(calls[0].args?.x).toBe(18)
    expect(calls[0].args?.height).toBe(WINDOW.DEFAULT_HEIGHT)
    expect(calls[0].args?.y).toBe(780 - WINDOW.DEFAULT_HEIGHT)
    perfSpy.mockRestore()
    h.wrapper.unmount()
  })
})
