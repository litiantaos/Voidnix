import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'

// View.vue 的 onActivated 恢复流依赖 Tauri invoke / 事件监听 / app store，全部 mock。
// 场景核心：升级中退出（KeepAlive LRU 驱逐 → 组件销毁）后重进，
// brew_run_state 返回 Some 时应渲染列表 + 恢复运行态，而非阻断为加载态等操作结束。
const mocks = vi.hoisted(() => {
  const listeners = new Map<string, (e: { payload: unknown }) => void>()
  return {
    listeners,
    invoke: vi.fn(),
    listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
      listeners.set(event, handler)
      return () => listeners.delete(event)
    }),
    showStatus: vi.fn(),
    openSubview: vi.fn(),
  }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  Channel: class {
    onmessage: unknown = null
  },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }))
vi.mock('@/utils/tauri', () => ({ isTauri: true }))
vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    searchQuery: '',
    showStatus: mocks.showStatus,
    openSubview: mocks.openSubview,
  }),
}))

import '@/locales'
import './locales'
import View from './View.vue'

interface BrewStatusPayload {
  version: string
  packages: {
    name: string
    kind: string
    desc: string
    version: string
    new_version: string
  }[]
  has_update: boolean
  refreshing: boolean
}

let runState: { operation: string; step: string } | null = null
let brewStatus: () => Promise<BrewStatusPayload> = () => Promise.resolve(statusPayload())

function statusPayload(): BrewStatusPayload {
  return {
    version: '4.4.0',
    packages: [
      {
        name: 'git',
        kind: 'formula',
        desc: 'Distributed VCS',
        version: '2.40.0',
        new_version: '2.43.0',
      },
    ],
    has_update: true,
    refreshing: false,
  }
}

function mountHost() {
  const show = ref(true)
  const wrapper = mount(
    defineComponent({
      setup: () => () => h(KeepAlive, () => (show.value ? h(View) : null)),
    }),
  )
  return { wrapper, show }
}

async function flush(times = 10) {
  for (let i = 0; i < times; i++) await nextTick()
}

beforeEach(() => {
  mocks.listeners.clear()
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((cmd: string) => {
    if (cmd === 'brew_run_state') return Promise.resolve(runState)
    if (cmd === 'brew_status') return brewStatus()
    if (cmd === 'brew_services') return Promise.resolve([])
    return Promise.resolve(null)
  })
  runState = null
  brewStatus = () => Promise.resolve(statusPayload())
})

describe('homebrew View 运行态恢复', () => {
  it('空闲进入：拉数据渲染列表，无运行态', async () => {
    const { wrapper } = mountHost()
    await flush()

    expect(wrapper.text()).toContain('git')
    expect(wrapper.text()).not.toContain('加载中')
    expect(wrapper.text()).not.toContain('升级中')
    // 卸载清理：BaseList 的 document 级回车监听（执行即消费）泄漏会吞掉后续用例的回车
    wrapper.unmount()
  })

  it('升级中退出后重进：列表保持可浏览 + 按钮恢复当前步骤，不阻断为加载态', async () => {
    // 第一阶段：空闲进入（列表已拉取）
    const first = mountHost()
    await flush()
    first.wrapper.unmount()

    // 第二阶段：后台升级仍在进行，重进（全新实例，模拟 KeepAlive LRU 驱逐卸载）
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    const second = mountHost()
    await flush()

    const text = second.wrapper.text()
    // 列表与状态行保持渲染（可浏览），进度经按钮位滚动显示当前步骤
    expect(text).toContain('git')
    expect(text).toContain('Homebrew')
    expect(text).toContain('升级中')
    // 无独立进度卡
    expect(second.wrapper.find('.soft-card').exists()).toBe(false)
    expect(text).not.toContain('加载中')
    // 运行中禁用更新按钮，防重复触发
    expect(second.wrapper.html()).toContain('disabled')
    second.wrapper.unmount()
  })

  it('一键更新：按钮位滚动显示进度详情，line 输出不渲染，完成后回常态', async () => {
    let resolveRun: () => void = () => {}
    const runPromise = new Promise<void>((r) => (resolveRun = r))
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_status') return Promise.resolve(statusPayload())
      if (cmd === 'brew_services') return Promise.resolve([])
      if (cmd === 'brew_run') return runPromise
      return Promise.resolve(null)
    })
    const { wrapper } = mountHost()
    await flush()

    const updateBtn = wrapper.findAll('button').find((b) => b.text() === '更新')
    expect(updateBtn?.exists()).toBe(true)
    await updateBtn!.trigger('click')
    await flush()

    // 点击即运行态：旋转 + 占位文案，无独立进度卡
    expect(wrapper.find('i.animate-spin').exists()).toBe(true)
    expect(wrapper.find('.soft-card').exists()).toBe(false)

    // 从 brew_run 调用参数捕获 Channel，模拟 Rust 流式事件
    const runCall = mocks.invoke.mock.calls.find((c) => c[0] === 'brew_run')
    const onEvent = (runCall?.[1] as { onEvent: { onmessage: unknown } })?.onEvent
    const fire = onEvent?.onmessage as (e: { kind: string; text: string }) => void

    fire({ kind: 'step', text: 'update' })
    await flush()
    expect(wrapper.text()).toContain('拉取更新')

    fire({ kind: 'step', text: 'upgrade' })
    fire({ kind: 'line', text: '==> Upgrading 1 outdated package:' })
    await flush()
    // 表头行不误判为包名，显示步骤名
    expect(wrapper.text()).toContain('升级中')
    expect(wrapper.text()).not.toContain('outdated')

    // 下载行（ghcr blobs URL 段）即取包名——下载先于 Pouring，是最长阶段
    fire({
      kind: 'line',
      text: '==> Downloading https://ghcr.io/v2/homebrew/core/git/blobs/sha256:9f2c',
    })
    await flush()
    expect(wrapper.text()).toContain('git 0/1')

    // cask 升级行（Upgrading Cask <token>）不把 Cask 当包名
    fire({ kind: 'line', text: '==> Upgrading Cask google-chrome' })
    await flush()
    expect(wrapper.text()).toContain('google-chrome 0/1')

    fire({ kind: 'line', text: '==> Pouring git--2.43.0.arm64_sonoma.bottle.tar.gz' })
    await flush()
    // 当前包名 + 计数
    expect(wrapper.text()).toContain('git 0/1')

    fire({ kind: 'line', text: '🍺  git 2.40.0 -> 2.43.0' })
    await flush()
    // 🍺 行计数完成
    expect(wrapper.text()).toContain('git 1/1')
    // line 事件不产生任何终端输出
    expect(wrapper.text()).not.toContain('Pouring')

    // 完成：滚动详情退场、成功 toast
    resolveRun()
    await flush()
    expect(wrapper.text()).not.toContain('升级中')
    expect(wrapper.text()).not.toContain('git 1/1')
    expect(mocks.showStatus).toHaveBeenCalledWith('更新完成')
    wrapper.unmount()
  })

  it('后台操作完成：brew-run-done 清运行态并重拉最新状态', async () => {
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    const { wrapper } = mountHost()
    await flush()
    expect(wrapper.text()).toContain('升级中')

    runState = null
    const fresh = statusPayload()
    fresh.packages[0].new_version = ''
    fresh.has_update = false
    brewStatus = () => Promise.resolve(fresh)
    mocks.listeners.get('brew-run-done')?.({ payload: { operation: 'update_upgrade', step: '' } })
    await flush()

    expect(wrapper.text()).not.toContain('升级中')
    expect(wrapper.text()).not.toContain('2.43.0')
    wrapper.unmount()
  })

  it('查询返回过期 Some（done 事件先于响应到达）：丢弃运行态恢复，不卡死', async () => {
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    // brew_run_state 响应挂起：期间 done 事件先到（事件与 invoke 响应投递通道不同，无顺序保证）
    let resolveState: (v: unknown) => void = () => {}
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return new Promise((r) => (resolveState = r))
      if (cmd === 'brew_status') return brewStatus()
      if (cmd === 'brew_services') return Promise.resolve([])
      return Promise.resolve(null)
    })
    const { wrapper } = mountHost()
    await flush()

    // 操作结束：done 先于查询响应到达，监听器已清态并重拉
    runState = null
    mocks.listeners.get('brew-run-done')?.({ payload: null })
    await flush()

    // 过期的 Some 响应此时才到达：不得恢复运行态
    resolveState({ operation: 'update_upgrade', step: 'upgrade' })
    await flush()

    expect(wrapper.text()).toContain('git')
    expect(wrapper.text()).not.toContain('升级中')
    wrapper.unmount()
  })

  it('恢复态拉取与完成重拉并发：过期响应不覆盖最新结果', async () => {
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    // 第一轮（onActivated 恢复态拉取）挂起不返回
    let resolveFirst: (v: BrewStatusPayload) => void = () => {}
    const first = new Promise<BrewStatusPayload>((r) => {
      resolveFirst = r
    })
    brewStatus = () => first
    const { wrapper } = mountHost()
    await flush()

    // 完成事件在第一轮在途时触发第二轮（最新数据）
    runState = null
    const fresh = statusPayload()
    fresh.version = '4.5.0'
    brewStatus = () => Promise.resolve(fresh)
    mocks.listeners.get('brew-run-done')?.({ payload: null })
    await flush()
    expect(wrapper.text()).toContain('4.5.0')

    // 过期的第一轮此时才返回：旧数据不得落盘
    resolveFirst(statusPayload())
    await flush()
    expect(wrapper.text()).toContain('4.5.0')
    expect(wrapper.text()).not.toContain('加载中')
    wrapper.unmount()
  })

  it('后台元数据刷新完成：按钮先短暂消失（fetch 在途），落盘新状态后恢复更新按钮', async () => {
    // 时间线：进入视图时元数据陈旧（旧元数据下无 outdated）→ 后台 brew update
    // → done → 重拉返回新元数据（有 outdated）。fetch 在途期间按钮消失是已知窗口，
    // 但落盘 has_update=true 后按钮必须恢复显示（回车触发的前提与按钮可见性同源）。
    let doneFired = false
    let resolveFresh: (v: BrewStatusPayload) => void = () => {}
    const freshStatusPromise = new Promise<BrewStatusPayload>((r) => {
      resolveFresh = r
    })
    const stalePayload: BrewStatusPayload = {
      version: '4.4.0',
      packages: [{ name: 'git', kind: 'formula', desc: '', version: '2.40.0', new_version: '' }],
      has_update: false,
      refreshing: true,
    }
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_services') return Promise.resolve([])
      if (cmd === 'brew_status') {
        if (!doneFired) return Promise.resolve(stalePayload)
        return freshStatusPromise
      }
      return Promise.resolve(null)
    })

    const { wrapper } = mountHost()
    await flush()
    // 后台刷新中：按钮旋转显示「拉取更新」
    expect(wrapper.text()).toContain('拉取更新')

    // 后台 update 完成：done 先清运行态（按钮消失窗口），重拉挂起
    doneFired = true
    mocks.listeners.get('brew-run-done')?.({ payload: null })
    await flush()
    expect(wrapper.text()).not.toContain('拉取更新')

    // 新元数据落盘：有 outdated → 按钮必须恢复为「更新」
    resolveFresh({
      version: '4.4.0',
      packages: [
        { name: 'git', kind: 'formula', desc: '', version: '2.40.0', new_version: '2.43.0' },
      ],
      has_update: true,
      refreshing: false,
    })
    await flush()
    // 精确断言按钮文案（「拉取更新」也含「更新」子串，须用按钮元素文本反证）
    const updateBtn = wrapper.findAll('button').find((b) => b.text() === '更新')
    expect(updateBtn?.exists()).toBe(true)
    wrapper.unmount()
  })

  it('详情返回列表（KeepAlive 重激活）：重拉在途保留缓存列表，不闪 spinner', async () => {
    let hang = false
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_status') {
        if (!hang) return Promise.resolve(statusPayload())
        return new Promise<BrewStatusPayload>(() => {})
      }
      if (cmd === 'brew_services') return Promise.resolve([])
      return Promise.resolve(null)
    })
    const { wrapper, show } = mountHost()
    await flush()
    expect(wrapper.text()).toContain('git')

    // 模拟进详情（列表 deactivate）再返回：重拉挂起在途，缓存列表应保持可见
    show.value = false
    await flush()
    hang = true
    show.value = true
    await flush()
    expect(wrapper.text()).toContain('git')
    wrapper.unmount()
  })
})

describe('homebrew View 界面', () => {
  it('服务行无左侧图标、启停按钮为正常按钮组件；有更新的包名不带强调色', async () => {
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_status') return Promise.resolve(statusPayload())
      if (cmd === 'brew_services') return Promise.resolve([{ name: 'redis', status: 'started' }])
      return Promise.resolve(null)
    })
    const { wrapper } = mountHost()
    await flush()

    // 服务行左侧图标已移除
    expect(wrapper.html()).not.toContain('i-ri-flashlight-line')
    expect(wrapper.html()).not.toContain('i-ri-shut-down-line')
    // 右侧按钮走正常按钮组件（soft-chip 面，非 ghost 覆写），图标同为满幅圆形线稿（视觉等大）
    const stop = wrapper.find('[title="停止"]')
    expect(stop.exists()).toBe(true)
    expect(stop.classes()).toContain('soft-chip')
    expect(stop.classes()).not.toContain('ui-btn-ghost')
    expect(stop.classes()).not.toContain('!w-7')
    expect(stop.find('i').classes()).toContain('i-ri-stop-circle-line')
    expect(wrapper.find('[title="重启"] i').classes()).toContain('i-ri-restart-line')
    // 有更新的包名标题不带 accent 色，版本号告警色保留
    const gitTitle = wrapper.findAll('div').find((d) => d.text() === 'git')
    expect(gitTitle?.classes()).not.toContain('text-accent')
    expect(wrapper.html()).toContain('text-warning')
    wrapper.unmount()
  })

  it('服务行回车：进入包详情，不触发启停', async () => {
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_status') return Promise.resolve(statusPayload())
      if (cmd === 'brew_services')
        return Promise.resolve([
          { name: 'redis', status: 'started' },
          { name: 'postgresql@16', status: 'stopped' },
        ])
      return Promise.resolve(null)
    })
    const { wrapper } = mountHost()
    await flush()

    // 选中下移到首个服务行（redis，不在包列表中 → formula 兜底）后回车
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await flush()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flush()

    expect(mocks.openSubview).toHaveBeenCalledWith('detail', false)
    expect(JSON.parse(sessionStorage.getItem('homebrew:detail') ?? '{}')).toMatchObject({
      name: 'redis',
      kind: 'formula',
    })
    expect(mocks.invoke).not.toHaveBeenCalledWith('brew_run', expect.anything())
    wrapper.unmount()
  })
})
