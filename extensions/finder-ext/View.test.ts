import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'

/** 进入模式判定回归测试：访达快捷键 vs 应用界面。
 *
 * 快捷键进入时 makeToggleHandler 同步写 `entryViaShortcut` + `reactivateTick`，
 * 模式判定分散在两条链上：View 已激活（窗口隐藏重呼，无 onActivated 跟进）由 tick
 * watch 消费标记；View 停用（从主界面 / 其它扩展进入）由 onActivated 消费。
 * 若 tick watch 无条件消费，onActivated 随后到达只会读到 false → 误判浏览模式
 * （首次快捷键进入识别不到访达选区 / 媒体入口）。本测试用 KeepAlive 宿主复刻
 * 「停用 → 快捷键重激活」时序，锁死该判定。 */
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(async (_cmd: string) => [] as string[]),
  appStore: {
    activeExtId: 'finder-ext' as string | null,
    extensionReturnExtId: null as string | null,
    searchQuery: '',
    openSubview: vi.fn(),
    setActiveExtension: vi.fn(),
    showStatus: vi.fn(),
  },
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@/stores/app', () => ({
  useAppStore: () => mocks.appStore,
  // ./index 顶层经 defineExtension 引用，仅注册不执行
  makeToggleHandler: () => () => {},
  toastAndHide: vi.fn(),
}))
vi.mock('@/stores/settings', () => ({
  useSettingsStore: () => ({ getShortcutOverride: () => undefined, setShortcutOverride: vi.fn() }),
}))
// config.ts（经 OpenWithView → apps.ts 间接载入）落盘依赖 Tauri 宿主，单测改纯内存默认值
vi.mock('@/runtime/storage', () => ({
  defineConfig: (_key: string, defaults: Record<string, unknown>) => ({ ...defaults }),
}))

import './locales'
import { entryViaShortcut, reactivateTick } from './index'
import View from './View.vue'
import type { SettingItem } from '@/types/settings'

/** BaseSettingsList 替身：把 items 的 id 序列暴露到 DOM，供断言模式（浏览 / 上下文）。 */
const SettingsListStub = defineComponent({
  name: 'BaseSettingsList',
  props: { items: { type: Array, required: true }, shortcutId: String },
  setup(props) {
    return () =>
      h('div', {
        'data-ids': (props.items as SettingItem[]).map((i) => i.id).join('|'),
      })
  },
})

/** 复刻 ContentView 的 KeepAlive 宿主：active=false 让 View 停用（缓存不卸载）。 */
function mountCachedView() {
  const active = ref(true)
  const Host = defineComponent({
    setup: () => () => h(KeepAlive, null, { default: () => (active.value ? h(View) : null) }),
  })
  const wrapper = mount(Host, { global: { stubs: { BaseSettingsList: SettingsListStub } } })
  return { wrapper, active }
}

function itemIds(wrapper: ReturnType<typeof mount>): string[] {
  const raw = wrapper.get('[data-ids]').attributes('data-ids') ?? ''
  return raw ? raw.split('|') : []
}

/** 选区探测次数（finder_selected_paths 调用数）。 */
function probeCount(): number {
  return mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'finder_selected_paths').length
}

/** 快捷键进入：makeToggleHandler 顺序 = setActiveExtension（触发激活）→ 写标记 → tick。 */
async function enterViaShortcut(active: { value: boolean }) {
  active.value = true
  entryViaShortcut.value = true
  reactivateTick.value++
  await nextTick()
  await flushPromises()
}

describe('finder-ext 进入模式判定', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.appStore.activeExtId = 'finder-ext'
    entryViaShortcut.value = false
    reactivateTick.value = 0
  })

  it('应用界面进入为浏览模式：全量目录（媒体 / 归档入口恒显），不探测选区', async () => {
    const { wrapper } = mountCachedView()
    await flushPromises()

    const ids = itemIds(wrapper)
    expect(ids).toContain('video_process')
    expect(ids).toContain('image_process')
    expect(ids).toContain('archive_process')
    expect(mocks.invoke).not.toHaveBeenCalledWith('finder_selected_paths')

    wrapper.unmount()
  })

  it('视图停用缓存后快捷键首按：onActivated 判为访达上下文并探测选区', async () => {
    const { wrapper, active } = mountCachedView()
    await flushPromises()

    // 切走 → View 停用（KeepAlive 缓存）
    active.value = false
    await nextTick()

    // 快捷键重入：不能由 tick watch 提前消费标记（否则 onActivated 误判浏览模式）
    await enterViaShortcut(active)

    const ids = itemIds(wrapper)
    // 上下文模式无选区：仅通用动作，媒体 / 归档入口不出现（archive 入口需有选区）
    expect(ids).toEqual([
      'open_with',
      'copy_path',
      'open_terminal',
      'new_file',
      'toggle_hidden',
      'finder-ext',
    ])
    // 探测确实执行
    expect(mocks.invoke).toHaveBeenCalledWith('finder_selected_paths')

    wrapper.unmount()
  })

  it('已激活态快捷键重呼后退出、再经应用界面进入：残留意图不得误判上下文', async () => {
    const { wrapper, active } = mountCachedView()
    await flushPromises()

    // 已激活态下快捷键重呼（窗口隐藏后再呼出）：tick watch 切上下文，无 onActivated 消费意图
    await enterViaShortcut(active)
    expect(itemIds(wrapper)).not.toContain('video_process')
    expect(probeCount()).toBe(1)

    // 退出扩展（视图停用）→ 再经应用界面进入：停用复位使本次为浏览模式，且不重复探测
    active.value = false
    await nextTick()
    active.value = true
    await nextTick()
    await flushPromises()

    expect(itemIds(wrapper)).toContain('video_process')
    expect(probeCount()).toBe(1)

    wrapper.unmount()
  })

  it('二级界面让位后快捷键重入：保持上下文、重读选区且不泄漏进入标记', async () => {
    const { wrapper, active } = mountCachedView()
    await flushPromises()

    // 快捷键进入上下文 → 打开「用 App 打开」二级界面（contextResumable 置位）
    await enterViaShortcut(active)
    const openWith = (wrapper.getComponent(SettingsListStub).props('items') as SettingItem[]).find(
      (i) => i.id === 'open_with',
    )
    if (openWith?.type === 'action') openWith.action()
    await nextTick()

    // 二级界面让位：mainView 停用（KeepAlive 缓存）
    active.value = false
    await nextTick()

    // 快捷键重入：清 subview 回面板根，contextResumable 分支消费标记并重读访达
    await enterViaShortcut(active)
    expect(itemIds(wrapper)).not.toContain('video_process')
    expect(probeCount()).toBe(2)

    // 再切走（停用）后从应用界面进入：标记已消费 → 浏览模式（未泄漏为快捷键进入）
    active.value = false
    await nextTick()
    active.value = true
    await nextTick()
    await flushPromises()

    expect(itemIds(wrapper)).toContain('video_process')

    wrapper.unmount()
  })
})
