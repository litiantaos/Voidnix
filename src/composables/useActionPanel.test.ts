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

  /** 模拟 BaseDialog 的 isModalDialogOpen 探测判据（Teleport body + aria-modal） */
  function mountModalDialog() {
    const dialog = document.createElement('div')
    dialog.setAttribute('role', 'dialog')
    dialog.setAttribute('aria-modal', 'true')
    document.body.appendChild(dialog)
    return dialog
  }

  it('模态弹窗打开期间 Enter 让位弹窗（不 confirm、不拦截），面板保留；弹窗关闭后恢复响应', async () => {
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

    // 弹窗出现（如菜单栏检查更新唤起 UpdateDialog）：Enter / Cmd+Enter toggle 均让位，
    // 面板不关（非模态浮层，收束交给弹窗期间的外点关闭）
    const dialog = mountModalDialog()
    pressEnter()
    pressCmdEnter()
    expect(onSelect).not.toHaveBeenCalled()
    expect(openMirror.value).toBe(true)

    // 弹窗关闭：面板键盘恢复响应
    dialog.remove()
    pressEnter()
    expect(onSelect).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('模态弹窗打开期间外点关闭面板：不回焦搜索框（BaseDialog @keydown 绑弹窗根，焦点出走即死键）', async () => {
    const searchInput = document.createElement('input')
    searchInput.id = 'main-search-input'
    document.body.appendChild(searchInput)

    const panelEl = document.createElement('div')
    const openMirror = ref(false)
    const PanelHost = defineComponent({
      name: 'PanelHost',
      setup() {
        const panel = useActionPanel({
          panelRef: ref<HTMLElement | undefined>(panelEl),
          getItems: () => [{ type: 'item' as const, key: 'act', label: 'action' }],
          onSelect: () => {},
          canOpen: () => true,
        })
        watch(panel.open, (v) => (openMirror.value = v), { immediate: true })
        return () => h('div')
      },
    })
    const wrapper = mount(PanelHost)

    // 面板先开，弹窗后至（面板开着时检查更新唤起 UpdateDialog 的场景时序）
    pressCmdEnter()
    await nextTick()
    expect(openMirror.value).toBe(true)

    const dialog = mountModalDialog()
    const dialogBtn = document.createElement('button')
    dialog.appendChild(dialogBtn)
    dialogBtn.focus()

    // 点击弹窗内容（面板外）：面板关闭但焦点留在弹窗按钮上，不抢焦搜索框
    dialogBtn.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }))
    await nextTick()
    expect(openMirror.value).toBe(false)
    expect(document.activeElement).toBe(dialogBtn)

    dialog.remove()
    searchInput.remove()
    wrapper.unmount()
  })
})
