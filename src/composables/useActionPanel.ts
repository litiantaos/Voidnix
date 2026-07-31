import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import type { Ref } from 'vue'
import { wrapIndex } from '@/utils/dom'
import type { PanelItem } from '@/components/ui/BaseDropdownItems.vue'

interface UseActionPanelOptions {
  /** 浮层根元素 ref（消费者创建并传入，用于 focus 与外点关闭判定） */
  panelRef: Ref<HTMLElement | undefined>
  /** 动态菜单项（每次打开/导航时重新求值，支持条件项） */
  getItems: () => PanelItem[]
  /** 选中动作回调（点击 / 回车确认统一入口） */
  onSelect: (key: string | number) => void
  /** 是否允许打开（业务守卫 + 目标准备，如设置 menuTarget）。返回 true 后 composable 自动打开。
   *  isComposing 由 composable 在键盘通道拦截，无需消费者判；右键 toggle 时已开则直接关闭、不调用此函数 */
  canOpen: () => boolean
  /** 打开前异步预处理（如拉元数据、设置目标项）；菜单项在此后求值 */
  beforeOpen?: () => Promise<void> | void
}

/**
 * Cmd+Enter / 右键 动作浮层的通用逻辑（右下角 dropdown-panel + BaseDropdownItems）：
 * - menuIndex / selectableIndices / moveMenu / confirmMenu 键盘导航
 * - toggleOpen 右键入口（已开则关，否则 canOpen → openFor）
 * - onDocKey capture-phase 拦截（ArrowUp/Down/Enter/Esc）+ 外点关闭
 * - document 监听器随生命周期自注册/注销
 *
 * 消费者负责：getItems 业务项、onSelect 动作分派、canOpen 打开条件 + 目标准备、beforeOpen 预处理
 * 消费者保留浮层模板（Teleport + Transition + BaseDropdownItems）与其他独立交互（如预览 Esc）
 */
export function useActionPanel(opts: UseActionPanelOptions) {
  const open = ref(false)
  const menuIndex = ref(-1)

  const selectableIndices = computed(() =>
    opts
      .getItems()
      .map((it, i) => (it.type === 'item' && !it.disabled ? i : -1))
      .filter((i) => i >= 0),
  )

  async function openFor() {
    if (opts.beforeOpen) await opts.beforeOpen()
    menuIndex.value = selectableIndices.value[0] ?? -1
    open.value = true
    nextTick(() => opts.panelRef.value?.focus())
  }

  /** 右键 toggle：面板已开则关闭，否则经 canOpen 守卫 + 目标准备后打开（Cmd+Enter 同款流程） */
  async function toggleOpen() {
    if (open.value) {
      close()
      return
    }
    if (!opts.canOpen()) return
    await openFor()
  }

  function close() {
    open.value = false
    nextTick(() => document.getElementById('main-search-input')?.focus())
  }

  function moveMenu(dir: 1 | -1) {
    const ids = selectableIndices.value
    if (ids.length === 0) return
    const cur = Math.max(0, ids.indexOf(menuIndex.value))
    menuIndex.value = ids[wrapIndex(cur, ids.length, dir === 1 ? 'down' : 'up')]
  }

  function confirmMenu() {
    const item = opts.getItems()[menuIndex.value]
    if (!item || item.type !== 'item' || item.disabled || !item.key) return
    opts.onSelect(item.key)
  }

  function onMenuClick(i: number) {
    const item = opts.getItems()[i]
    if (!item || item.type !== 'item' || item.disabled || !item.key) return
    opts.onSelect(item.key)
  }

  function onDocKey(e: KeyboardEvent) {
    if (e.isComposing) return
    if (open.value) {
      if (e.key === 'Escape' || (e.key === 'Enter' && e.metaKey)) {
        e.preventDefault()
        e.stopPropagation()
        close()
      } else if (e.key === 'Enter') {
        e.preventDefault()
        e.stopPropagation()
        confirmMenu()
      } else if (e.key === 'ArrowDown') {
        e.preventDefault()
        e.stopPropagation()
        moveMenu(1)
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        e.stopPropagation()
        moveMenu(-1)
      }
      return
    }
    if (e.key === 'Enter' && e.metaKey && opts.canOpen()) {
      e.preventDefault()
      e.stopPropagation()
      void openFor()
    }
  }

  function onDocMouseDown(e: MouseEvent) {
    // 右键由 contextmenu 通道处理（结果项右键重定向面板），不在此关闭
    if (e.button === 2) return
    if (open.value && opts.panelRef.value && !opts.panelRef.value.contains(e.target as Node)) {
      close()
    }
  }

  onMounted(() => {
    document.addEventListener('keydown', onDocKey, true)
    document.addEventListener('mousedown', onDocMouseDown)
  })
  onBeforeUnmount(() => {
    document.removeEventListener('keydown', onDocKey, true)
    document.removeEventListener('mousedown', onDocMouseDown)
  })

  return {
    open,
    menuIndex,
    selectableIndices,
    toggleOpen,
    close,
    moveMenu,
    confirmMenu,
    onMenuClick,
  }
}
