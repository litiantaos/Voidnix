import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// isTauri 是模块级常量，须在 import 待测模块前 mock
vi.mock('@/utils/tauri', () => ({ isTauri: true }))

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }))

// 模块级单例（healing / lastKeydownAt / keydownArmed）逐用例重隔离
let ensureRenderPulse: () => Promise<void>
const loadModule = async () => {
  vi.resetModules()
  ;({ ensureRenderPulse } = await import('./render-pulse'))
}
const { CMD } = await import('@/commands')

/** rAF 队列 mock：冻结态不 flush（渲染时钟停摆），恢复态 flushRaf() 执行回调 */
let rafCallbacks: FrameRequestCallback[] = []
const flushRaf = () => {
  const cbs = rafCallbacks
  rafCallbacks = []
  cbs.forEach((cb) => cb(0))
}

const reloadMock = vi.fn()

/** 模拟用户按键（在场证据） */
const pressKey = () => document.dispatchEvent(new KeyboardEvent('keydown'))

/** 冻结一轮探测 + kick + 复核可见（默认 mock），返回 settle 后的 promise。
 *  probe1 超时后按键——keydown 监听挂载于探测链起点，真实在场按键落在探测窗口内 */
async function runFrozenRound(withKey = true) {
  const p = ensureRenderPulse()
  await vi.advanceTimersByTimeAsync(2000) // probe1 冻结 → kick（监听已挂载）
  if (withKey) pressKey() // 用户在场
  await vi.advanceTimersByTimeAsync(2000) // probe2 冻结 → 三道闸检查
  await p
}

beforeEach(async () => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-11-21T12:00:00'))
  rafCallbacks = []
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    rafCallbacks.push(cb)
    return rafCallbacks.length
  })
  vi.stubGlobal('location', { ...location, reload: reloadMock })
  invokeMock.mockReset()
  invokeMock.mockResolvedValue(undefined) // 默认：kick resolve、可见性复核 undefined 按可见处理
  sessionStorage.clear()
  await loadModule()
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('ensureRenderPulse', () => {
  it('渲染时钟正常（rAF 执行）：零自愈动作', async () => {
    const p = ensureRenderPulse()
    flushRaf() // rAF 立即执行（正常路径 ~16ms）
    await p
    expect(invokeMock).not.toHaveBeenCalled()
    expect(reloadMock).not.toHaveBeenCalled()
  })

  it('冻结 → kick 后恢复：不重载', async () => {
    invokeMock.mockResolvedValueOnce(undefined) // kick
    const p = ensureRenderPulse()
    await vi.advanceTimersByTimeAsync(2000) // probe1 超时（rAF 未执行）→ kick
    expect(invokeMock).toHaveBeenCalledWith(CMD.kickWebviewRender)
    flushRaf() // kick 生效，probe2 的 rAF 执行
    await p
    expect(reloadMock).not.toHaveBeenCalled()
  })

  it('冻结 → kick 无效 → 可见且在场 → 重载并落冷却时间戳', async () => {
    invokeMock.mockResolvedValueOnce(undefined) // kick
    invokeMock.mockResolvedValue(true) // isMainWindowVisible 复核
    await runFrozenRound()
    expect(reloadMock).toHaveBeenCalledTimes(1)
    expect(Number(sessionStorage.getItem('voidnix:render-revive-reloaded'))).toBe(Date.now())
  })

  it('探测期间正常关窗（复核不可见）：不重载——隐藏冻结是设计行为', async () => {
    invokeMock.mockResolvedValueOnce(undefined) // kick（Rust 侧可见性守卫拦截，前端不感知）
    invokeMock.mockResolvedValue(false) // isMainWindowVisible → false
    await runFrozenRound()
    expect(reloadMock).not.toHaveBeenCalled()
  })

  it('息屏/锁屏误判防护：窗口逻辑可见但零交互（无 keydown）→ 不重载', async () => {
    invokeMock.mockResolvedValueOnce(undefined) // kick
    invokeMock.mockResolvedValue(true) // 逻辑可见（WINDOW_VISIBLE）
    await runFrozenRound(false) // 不按键：屏幕 occlusion 停摆与冻结 bug JS 视角同构
    expect(reloadMock).not.toHaveBeenCalled()
  })

  it('在场证据有时效：按键 60s 后过期 → 不重载', async () => {
    invokeMock.mockResolvedValueOnce(undefined)
    // 第一轮：监听挂载 + probe1 后按键 + 恢复结束（零自愈残留）
    const warm = ensureRenderPulse()
    await vi.advanceTimersByTimeAsync(2000)
    pressKey()
    flushRaf()
    await warm
    await vi.advanceTimersByTimeAsync(61000) // 在场窗口流逝
    await runFrozenRound()
    expect(reloadMock).not.toHaveBeenCalled()
  })

  it('冷却期内不重载（防循环），过期后新的空白允许再次自愈', async () => {
    invokeMock.mockResolvedValue(true)
    await runFrozenRound() // 第一次 reload
    expect(reloadMock).toHaveBeenCalledTimes(1)

    await loadModule() // reload 后新页面（模块单例清零），sessionStorage 时间戳跨 reload 存活
    invokeMock.mockResolvedValue(true)
    await runFrozenRound() // 冷却期内（10min）的再次冻结：kick 跑但不重载
    expect(reloadMock).toHaveBeenCalledTimes(1)

    await loadModule()
    invokeMock.mockResolvedValue(true)
    await vi.advanceTimersByTimeAsync(11 * 60 * 1000) // 冷却过期（新空白伴随新的唤起+在场）
    await runFrozenRound()
    expect(reloadMock).toHaveBeenCalledTimes(2)
  })

  it('in-flight 期间重入幂等：只走一条自愈链', async () => {
    invokeMock.mockResolvedValueOnce(undefined) // kick
    const first = ensureRenderPulse()
    const second = ensureRenderPulse() // healing=true 直跳
    await vi.advanceTimersByTimeAsync(2000)
    flushRaf() // probe2 恢复
    await Promise.all([first, second])
    expect(invokeMock).toHaveBeenCalledTimes(1)
    expect(reloadMock).not.toHaveBeenCalled()
  })
})
