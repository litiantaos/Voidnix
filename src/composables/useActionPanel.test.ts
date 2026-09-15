import { describe, it, expect, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h, nextTick, KeepAlive, ref, watch } from 'vue'
import { useActionPanel } from './useActionPanel'

/**
 * KeepAlive 树内的面板消费者。open 镜像到外部 ref 供断言：真实面板经 Teleport 挂
 * body（deactivate 只移宿主 DOM，Teleport 内容残留——正是双面板根因），测试环境
 * Teleport-to-body 不落地，open=true 即等价面板渲染在 body；且 watch 跨 deactivate
 * 仍活跃（与被测监听同生命周期），镜像忠实。
 */
function mountKeepAliveHost(onSelect: () => void) {
  const openMirror = ref(false)
  const PanelHost = defineComponent({
    name: 'PanelHost',
    setup() {
      const panel = useActionPanel({
        panelRef: ref(),
        getItems: () => [{ type: 'item' as const, key: 'act', label: 'action' }],
        onSelect,
        canOpen: () => true,
      })
      watch(panel.open, (v) => (openMirror.value = v), { immediate: true })
      return () => h('div')
    },
  })
  const OtherHost = defineComponent({
    name: 'OtherHost',
    setup: () => () => h('div', { class: 'other' }),
  })
  const showPanel = ref(true)
  const wrapper = mount({
    setup: () => () =>
      h(KeepAlive, () =>
        showPanel.value ? h(PanelHost, { key: 'panel' }) : h(OtherHost, { key: 'other' }),
      ),
  })
  return { wrapper, showPanel, openMirror }
}

function pressCmdEnter() {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true }))
}

function pressEnter() {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
}

describe('useActionPanel', () => {
  it('deactivate 后面板关闭、文档级监听让位；activate 后恢复', async () => {
    const { wrapper, showPanel, openMirror } = mountKeepAliveHost(vi.fn())

    // 激活态：Cmd+Enter 打开面板
    pressCmdEnter()
    await nextTick()
    expect(openMirror.value).toBe(true)

    // 切走（KeepAlive deactivate）：面板立即关闭，body 无残留
    showPanel.value = false
    await nextTick()
    expect(openMirror.value).toBe(false)

    // deactivated 期间 Cmd+Enter 不再打开（与主界面面板同屏双开根因）
    pressCmdEnter()
    await nextTick()
    expect(openMirror.value).toBe(false)

    // 切回（activate）：恢复响应
    showPanel.value = true
    await nextTick()
    pressCmdEnter()
    await nextTick()
    expect(openMirror.value).toBe(true)
    wrapper.unmount()
  })

  it('deactivate 后 Enter 不触发 onSelect（confirm 打架防护）', async () => {
    const onSelect = vi.fn()
    const { wrapper, showPanel } = mountKeepAliveHost(onSelect)

    pressCmdEnter()
    await nextTick()
    pressEnter()
    expect(onSelect).toHaveBeenCalledTimes(1)

    // deactivated 期间面板已关：Enter 直接让位，不 confirm 不拦截
    showPanel.value = false
    await nextTick()
    pressEnter()
    expect(onSelect).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('openFor 异步边界期间宿主 deactivate：放弃打开（跨界面弹出防护）', async () => {
    // beforeOpen 挂起：构造「右键已触发 openFor → await 中切换扩展」的竞态窗口
    let releaseBeforeOpen: () => void = () => {}
    const gate = new Promise<void>((resolve) => (releaseBeforeOpen = resolve))
    const openMirror = ref(false)
    const PanelHost = defineComponent({
      name: 'PanelHost',
      setup() {
        const panel = useActionPanel({
          panelRef: ref(),
          getItems: () => [{ type: 'item' as const, key: 'act', label: 'action' }],
          onSelect: () => {},
          canOpen: () => true,
          beforeOpen: () => gate,
        })
        watch(panel.open, (v) => (openMirror.value = v), { immediate: true })
        return () => h('div')
      },
    })
    const OtherHost = defineComponent({ name: 'OtherHost', setup: () => () => h('div') })
    const showPanel = ref(true)
    const wrapper = mount({
      setup: () => () =>
        h(KeepAlive, () =>
          showPanel.value ? h(PanelHost, { key: 'panel' }) : h(OtherHost, { key: 'other' }),
        ),
    })

    // Cmd+Enter 进入 openFor，挂在 beforeOpen 的 await 上
    pressCmdEnter()
    await nextTick()
    expect(openMirror.value).toBe(false)

    // await 期间切换扩展（deactivate），随后 beforeOpen 才完成
    showPanel.value = false
    await nextTick()
    releaseBeforeOpen()
    // flushPromises（宏任务）排空 openFor 恢复链的全部微任务——单次 nextTick 会
    // 在 openFor 恢复前断言造成假绿
    await flushPromises()
    expect(openMirror.value).toBe(false)
    wrapper.unmount()
  })

  it('非 KeepAlive 树内消费者（ResultActionPanel 场景）不受生命周期钩子影响', async () => {
    const onSelect = vi.fn()
    const openMirror = ref(false)
    const PanelHost = defineComponent({
      name: 'PanelHost',
      setup() {
        const panel = useActionPanel({
          panelRef: ref(),
          getItems: () => [{ type: 'item' as const, key: 'act', label: 'action' }],
          onSelect,
          canOpen: () => true,
        })
        watch(panel.open, (v) => (openMirror.value = v), { immediate: true })
        return () => h('div')
      },
    })
    const wrapper = mount(PanelHost)

    pressCmdEnter()
    await nextTick()
    expect(openMirror.value).toBe(true)
    pressEnter()
    expect(onSelect).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
})
