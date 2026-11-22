import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// 跨扩展投递同步达回归：finder-ext 跳转（投递 + setActiveExtension 同一同步任务）后，
// image 视图首帧即消费输入；任务行常驻第一项，列表形状与投递时序无关——不会出现
// 「首帧只有 source 行、用户 ↓+Enter 误开系统文件选择器」的未定型窗口。
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))

import '@/locales'
import './locales'
import { useAppStore } from '@/stores/app'
import imageExt, { pendingInputPaths } from './index'
import ImageView from './View.vue'

/** setup 仅跑一次（window 监听幂等注册，重复跑会叠加监听器） */
let setupDone = false
async function ensureSetup() {
  if (setupDone) return
  setupDone = true
  await imageExt.setup?.()
}

/** 模拟 LRU 驱逐重挂载宿主：v-if 写在 KeepAlive 自身（组件真销毁），alive 翻真即重挂载 */
function mountHost() {
  const alive = ref(false)
  const wrapper = mount(
    defineComponent({
      setup: () => () => (alive.value ? h(KeepAlive, () => h(ImageView)) : null),
    }),
  )
  return { wrapper, alive }
}

/** 模拟 finder-ext「图片处理」动作的投递：同页 CustomEvent 同步写入（数组，多选区全量） */
function deliver(paths: string[]) {
  window.dispatchEvent(new CustomEvent('image-pending-input-path', { detail: paths }))
}

/** 列表行 id 序（si-* 为 BaseListItem 根 id，DOM 序即列表序） */
function rowIds(wrapper: ReturnType<typeof mount>): string[] {
  return wrapper.findAll('[id^="si-"]').map((row) => row.attributes('id') ?? '')
}

async function flush(times = 10) {
  for (let i = 0; i < times; i++) await nextTick()
}

beforeEach(() => {
  setActivePinia(createPinia())
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((cmd: string) => {
    if (cmd === 'image_read_preview') return Promise.resolve('data:image/png;base64,iVBORw0KGgo=')
    if (cmd === 'pick_files') return Promise.resolve([])
    return Promise.resolve(null)
  })
  pendingInputPaths.value = []
})

describe('finder-ext → image 跨扩展同页投递', () => {
  it('setup 注册的 window 监听同步写入 pendingInputPaths（无 IPC 往返依赖）', async () => {
    await ensureSetup()
    deliver(['/tmp/pic.png'])
    // 同步断言：dispatch 返回时值已就位（旧 Tauri 事件总线需等 macrotask 回调）
    expect(pendingInputPaths.value).toEqual(['/tmp/pic.png'])
  })

  it('回归：LRU 驱逐重挂载（投递时未挂载），mount 即消费，首帧列表形状定型', async () => {
    await ensureSetup()
    const { wrapper, alive } = mountHost()
    await flush()

    // 投递与挂载同拍发生（对应 KeepAlive 驱逐后经跳转重挂载，watch immediate 消费）
    deliver(['/Users/x/照片.png'])
    alive.value = true
    await flush()

    // 任务行常驻第一项；投递后 source 行直接带图
    expect(rowIds(wrapper)).toEqual(['si-operations', 'si-tool', 'si-source', 'si-outputDir'])
    expect(pendingInputPaths.value).toEqual([])
    wrapper.unmount()
  })

  it('投递经 macrotask 到达：任务行常驻，首帧形状已定型，仅 source 内容更新', async () => {
    await ensureSetup()
    const appStore = useAppStore()
    const { wrapper, alive } = mountHost()
    alive.value = true
    await flush()

    // 投递未达的首帧：任务行已在，形状不再依赖 operations 行插入
    expect(rowIds(wrapper)).toEqual(['si-operations', 'si-tool', 'si-source', 'si-outputDir'])

    // 模拟旧 emit + listen：投递经 macrotask 到达（setActiveExtension 同步先行）
    setTimeout(() => {
      pendingInputPaths.value = ['/Users/x/a.jpg']
    }, 0)
    appStore.setActiveExtension('image')
    await nextTick()
    // 首帧形状稳定（无 operations 行插入空窗）
    expect(rowIds(wrapper)).toEqual(['si-operations', 'si-tool', 'si-source', 'si-outputDir'])
    await new Promise((r) => setTimeout(r, 0))
    await flush()
    // 投递到达后仍是同一形状，只更新 source 内容
    expect(rowIds(wrapper)).toEqual(['si-operations', 'si-tool', 'si-source', 'si-outputDir'])
    expect(wrapper.find('[id="si-source"]').text()).toContain('a.jpg')
    expect(pendingInputPaths.value).toEqual([])
    appStore.setActiveExtension(null)
    wrapper.unmount()
  })

  it('回归：KeepAlive 缓存存活（投递时已挂载），投递与激活同一 flush 内消费，单次渲染即终态', async () => {
    await ensureSetup()
    const appStore = useAppStore()
    const { wrapper, alive } = mountHost()
    alive.value = true
    await flush()

    // 跳转动作完整顺序：投递（同步）→ setActiveExtension（同步）→ 一次 flush
    deliver(['/Users/x/a.jpg'])
    appStore.setActiveExtension('image')
    // 仅一次 nextTick（对应跳转首帧）：operations 行必须已在——
    // 旧实现此刻列表仍为 [tool, source, outputDir]（IPC 未达），↓+Enter 会误中 source
    await nextTick()
    expect(rowIds(wrapper)).toEqual(['si-operations', 'si-tool', 'si-source', 'si-outputDir'])
    expect(pendingInputPaths.value).toEqual([])
    appStore.setActiveExtension(null)
    await flush()
    wrapper.unmount()
  })

  it('回归：多张投递自动切拼接模式，首帧列表含拼接参数行', async () => {
    await ensureSetup()
    const appStore = useAppStore()
    const { wrapper, alive } = mountHost()
    alive.value = true
    await flush()

    // 访达多选图片投递（选区序）：跳转首帧即拼接终态形状，参数行齐全
    deliver(['/Users/x/b.jpg', '/Users/x/a.png', '/Users/x/c.heic'])
    appStore.setActiveExtension('image')
    await nextTick()
    expect(rowIds(wrapper)).toEqual([
      'si-operations',
      'si-tool',
      'si-source',
      'si-direction',
      'si-resize',
      'si-gap',
      'si-outputDir',
    ])
    expect(pendingInputPaths.value).toEqual([])
    appStore.setActiveExtension(null)
    await flush()
    wrapper.unmount()
  })

  it('回归：多张投递整体替换共享集合，不残留前次投递列表', async () => {
    await ensureSetup()
    const appStore = useAppStore()
    const { wrapper, alive } = mountHost()
    alive.value = true
    await flush()

    // 首次投递进入拼接，视图保持活跃（缓存态）时再次投递：集合整体替换而非追加
    deliver(['/Users/x/keep1.png', '/Users/x/keep2.png'])
    appStore.setActiveExtension('image')
    await flush()
    deliver(['/Users/x/fresh1.png', '/Users/x/fresh2.png'])
    await flush()

    // source 行标题 = 替换后的「2 张图片」（追加则成 4 张）
    expect(wrapper.find('[id="si-source"]').text()).toContain('2 张图片')
    appStore.setActiveExtension(null)
    await flush()
    wrapper.unmount()
  })
})
