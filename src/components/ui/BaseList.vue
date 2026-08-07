<template>
  <div p="x-3" pb="3" flex="~ col" gap="1.5" role="listbox" aria-label="搜索结果">
    <template v-for="(item, i) in items" :key="itemKey(item, i)">
      <div
        v-if="
          groupField &&
          (i === 0 || getGroupValue(item) !== getGroupValue(items[i - 1])) &&
          (groupTitle ? groupTitle(getGroupValue(item)) : getGroupValue(item))
        "
        class="group-header"
      >
        <slot name="group-title" :group="getGroupValue(item)" :item="item" :index="i">
          {{ groupTitle ? groupTitle(getGroupValue(item)) : getGroupValue(item) }}
        </slot>
      </div>

      <div
        :ref="(el: unknown) => setItemRef(el, i)"
        role="option"
        :aria-selected="isItemSelected(i)"
        class="radius-panel"
        :class="{ 'ui-active': isItemSelected(i) }"
        @click="onItemClick(i, $event)"
        @dblclick="onItemDblClick(i)"
        @contextmenu="onItemContextMenu(i, $event)"
      >
        <slot name="item" :item="item" :index="i" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts" generic="T">
import { ref, watch, nextTick, onActivated, onDeactivated, onBeforeUnmount } from 'vue'
import { onKeyStroke } from '@/composables/events'
import { isComposing as isComposingCheck, isFormControl, wrapIndex } from '@/utils/dom'

// KeepAlive 软禁用：deactivate 后监听仍在，用 isActive 抑制响应
const isActive = ref(true)
onActivated(() => {
  isActive.value = true
})
onDeactivated(() => {
  isActive.value = false
})

const props = withDefaults(
  defineProps<{
    items: T[]
    selectedIndex?: number
    groupField?: keyof T | ((item: T) => string)
    groupTitle?: (group: string) => string
    multiSelect?: boolean
    selectedIds?: Set<string>
    idField?: string
    /** 是否激活键盘导航（由父组件根据扩展状态控制） */
    keyboardActive?: boolean
    /** IME 输入法合成状态（由父组件传入） */
    composing?: boolean
    /** ArrowUp/Down 在自定义输入框聚焦时是否仍导航（如翻译框）。
     *  全局搜索框由 data-list-execute 属性统一放行，无需此 prop。Enter 一律让出
     *  除非控件标记 data-list-execute。默认 false 保护设置页 input 编辑。 */
    navigateOnInput?: boolean
  }>(),
  {
    selectedIndex: 0,
    multiSelect: false,
    idField: 'id',
    keyboardActive: true,
    composing: false,
    navigateOnInput: false,
  },
)

const emit = defineEmits<{
  'update:selectedIndex': [index: number]
  'update:selectedIds': [ids: Set<string>]
  select: [index: number]
  execute: [item: T, index: number, event?: KeyboardEvent]
  contextmenu: [item: T, index: number, event: MouseEvent]
}>()

const localIndex = ref(props.selectedIndex)
watch(
  () => props.selectedIndex,
  (val) => {
    localIndex.value = val
  },
)

function setSelectedIndex(index: number) {
  localIndex.value = index
  emit('update:selectedIndex', index)
  emit('select', index)
}

// ── Refs ──
// 函数 ref 在节点卸载时会收到 null；必须同步释放旧 DOM 引用，否则搜索结果
// 缩短/替换后，itemRefs 会把整棵已脱离文档的节点树继续挂在 JS 堆上。
const itemRefs = ref<Array<HTMLElement | null>>([])
function setItemRef(el: unknown, index: number) {
  if (el) {
    itemRefs.value[index] = (el as { $el: HTMLElement }).$el || (el as HTMLElement)
    return
  }

  itemRefs.value[index] = null
  // 结果列表缩短时裁掉尾部空槽，避免数组本身随历史最大列表长度保留。
  while (itemRefs.value.length > 0 && itemRefs.value[itemRefs.value.length - 1] === null) {
    itemRefs.value.pop()
  }
}
onBeforeUnmount(() => {
  // 原地清空：比赋新数组（itemRefs.value = []）更利于 GC——
  // 旧数组引用链立即断开，而非等 ref 替换后旧数组被间接持有期间 DOM 节点仍可达
  itemRefs.value.length = 0
})
defineExpose({ selectedIndex: localIndex, setSelectedIndex, reveal })

// ── Multi-select ──
let anchorIndex = -1

function getId(item: T): string {
  return (item as Record<string, unknown>)[props.idField] as string
}

/// 列表 key：优先 idField，缺省回退 index（避免重排时 DOM 错位复用）
function itemKey(item: T, index: number): string | number {
  const id = (item as Record<string, unknown>)?.[props.idField]
  if (id != null && id !== '') return String(id)
  return index
}

function isMultiSelected(index: number): boolean {
  if (!props.multiSelect || !props.selectedIds) return false
  return props.selectedIds.has(getId(props.items[index]))
}

/// wrapper 是否高亮：单选焦点项 或 多选项
function isItemSelected(index: number): boolean {
  return localIndex.value === index || isMultiSelected(index)
}

function emitIds(ids: Set<string>) {
  emit('update:selectedIds', ids)
}

/// shift 范围选择：anchor → index 区间全选
function selectRangeTo(index: number) {
  if (anchorIndex < 0) anchorIndex = localIndex.value
  const [start, end] = [Math.min(anchorIndex, index), Math.max(anchorIndex, index)]
  const ids = new Set<string>()
  for (let i = start; i <= end; i++) ids.add(getId(props.items[i]))
  setSelectedIndex(index)
  emitIds(ids)
}

function onItemClick(index: number, e: MouseEvent) {
  if (props.multiSelect && (e.metaKey || e.ctrlKey || e.shiftKey)) {
    if (e.shiftKey) {
      selectRangeTo(index)
      return
    }
    const ids = new Set(props.selectedIds ?? [])
    if (ids.size === 0) {
      ids.add(getId(props.items[localIndex.value]))
      if (anchorIndex < 0) anchorIndex = localIndex.value
    }
    const id = getId(props.items[index])
    if (ids.has(id)) ids.delete(id)
    else ids.add(id)
    setSelectedIndex(index)
    emitIds(ids)
  } else {
    anchorIndex = index
    setSelectedIndex(index)
  }
}

function onItemDblClick(index: number) {
  emit('execute', props.items[index], index)
  if (props.multiSelect) emitIds(new Set())
}

/// 右键：选中该项并冒泡（消费者决定是否弹出菜单），抑制原生右键菜单
/// emit 延迟到 nextTick：setSelectedIndex 经 props 传播（flush）后再触发消费者
/// —— 全局 ResultActionPanel 的 canOpen 读 props.selectedIndex，同步 emit时 prop 尚未刷新
function onItemContextMenu(index: number, e: MouseEvent) {
  e.preventDefault()
  anchorIndex = index
  setSelectedIndex(index)
  nextTick(() => emit('contextmenu', props.items[index], index, e))
}

// ── Keyboard 守卫 ──

/** 模态弹窗打开时列表让出全部快捷键（设置页 BaseDialog 等；焦点在 BUTTON 上也不会再抢 ↑↓） */
function isModalDialogOpen(): boolean {
  return !!document.querySelector('[role="dialog"][aria-modal="true"]')
}

/// 公共守卫：未激活 / IME 合成中 / 模态弹窗打开 不响应
function canNavigate(e: KeyboardEvent): boolean {
  if (!isActive.value || !props.keyboardActive) return false
  if (props.composing || isComposingCheck(e)) return false
  if (isModalDialogOpen()) return false
  return true
}

/// ArrowUp/Down 让出判断：搜索框（data-list-execute）始终放行；其余表单控件按 navigateOnInput
function shouldYieldNavigation(): boolean {
  const active = document.activeElement as Element | null
  if (!isFormControl(active, { settingsControl: true })) return false
  if (active?.hasAttribute('data-list-execute')) return false
  return !props.navigateOnInput || props.items.length === 0
}

/// Enter 让出判断：表单控件聚焦时一律让出，除非控件显式委托（data-list-execute）
function shouldYieldExecution(): boolean {
  const active = document.activeElement as Element | null
  if (!isFormControl(active, { settingsControl: true })) return false
  return !active?.hasAttribute('data-list-execute')
}

// ── Keyboard ──
onKeyStroke(['ArrowDown', 'ArrowUp'], (e) => {
  if (!canNavigate(e)) return
  if (shouldYieldNavigation()) return

  const direction = e.key === 'ArrowDown' ? 'down' : 'up'

  // shift 范围多选
  if (props.multiSelect && e.shiftKey) {
    e.preventDefault()
    const next =
      direction === 'down'
        ? Math.min(localIndex.value + 1, props.items.length - 1)
        : Math.max(localIndex.value - 1, 0)
    if (next !== localIndex.value) selectRangeTo(next)
    return
  }

  e.preventDefault()
  if (props.items.length > 0) {
    const next = wrapIndex(localIndex.value, props.items.length, direction)
    setSelectedIndex(next)
    anchorIndex = next
    if ((props.selectedIds?.size ?? 0) > 0) emitIds(new Set())
  }
})

if (props.multiSelect) {
  onKeyStroke('a', (e) => {
    if (!canNavigate(e)) return
    if (!(e.metaKey || e.ctrlKey)) return
    e.preventDefault()
    emitIds(new Set(props.items.map((item) => getId(item))))
  })

  // 有多选项时 ESC 先清选择（不退出扩展）：子组件 onMounted 先于父，listener 注册在
  // useResultNavigation 之前，stopImmediatePropagation 阻断后者同 target bubble listener。
  onKeyStroke('Escape', (e) => {
    if (!canNavigate(e)) return
    if ((props.selectedIds?.size ?? 0) === 0) return
    e.preventDefault()
    e.stopImmediatePropagation()
    emitIds(new Set())
  })
}

onKeyStroke('Enter', (e) => {
  if (!canNavigate(e)) return
  if (shouldYieldExecution()) return
  // 按钮聚焦时 Enter 由按钮自身 click 处理
  if (document.activeElement?.tagName === 'BUTTON') return
  e.preventDefault()
  if (props.items.length > 0) {
    emit('execute', props.items[localIndex.value], localIndex.value, e)
    if (props.multiSelect) emitIds(new Set())
  }
})

// ── Scroll ──
function findScrollContainer(el: HTMLElement): HTMLElement | null {
  return el.closest('.overflow-y-auto, .overflow-auto') as HTMLElement | null
}

/// 读 scroll-padding-*（ContentView：top = chrome；bottom = CONTENT_INSET；无则 0）
function scrollPadding(container: HTMLElement, edge: 'Top' | 'Bottom'): number {
  const n = parseFloat(getComputedStyle(container)[`scrollPadding${edge}`])
  return Number.isFinite(n) ? n : 0
}

/// 抑制 watch 的瞬时滚动，reveal 定位时由 smooth 滚动接管
let suppressScroll = false

watch(localIndex, async (index) => {
  await nextTick()
  if (suppressScroll) return
  const el = itemRefs.value[index]
  if (!el) return
  const container = findScrollContainer(el)
  if (!container) return

  // 分组首项：连同标题一并滚入视野
  const isFirstInGroup =
    index === 0 ||
    (!!props.groupField &&
      getGroupValue(props.items[index]) !== getGroupValue(props.items[index - 1]))
  const topElement =
    isFirstInGroup && el.previousElementSibling ? (el.previousElementSibling as HTMLElement) : el

  const topInset = scrollPadding(container, 'Top')
  // bottom 无声明时回退 12（与列表 pb-3 / 全局 p-3 一致），避免贴底时下边距小于两侧
  const bottomInset = scrollPadding(container, 'Bottom') || 12
  const elRectTop = topElement.getBoundingClientRect().top
  const elRectBottom = el.getBoundingClientRect().bottom
  const containerRect = container.getBoundingClientRect()
  // 顶部可见区 = 容器顶 + chrome；底部 = 容器底 − scroll-padding-bottom
  const visibleTop = containerRect.top + topInset
  const visibleBottom = containerRect.bottom - bottomInset

  if (elRectBottom > visibleBottom) {
    container.scrollTop += elRectBottom - visibleBottom
  } else if (elRectTop < visibleTop) {
    container.scrollTop -= visibleTop - elRectTop
  }
})

/// 定位到指定项：高亮选中（同步导航索引）+ 平滑滚动居中
function reveal(index: number) {
  suppressScroll = true
  setSelectedIndex(index)
  // watch 的瞬时滚动在本轮微任务被 suppressScroll 拦截；下一宏任务复位并 smooth 滚动
  setTimeout(() => {
    suppressScroll = false
    void scrollIntoCenter(index)
  })
}

async function scrollIntoCenter(index: number) {
  await nextTick()
  const el = itemRefs.value[index]
  if (!el) return
  const container = findScrollContainer(el)
  if (!container) return
  const topInset = scrollPadding(container, 'Top')
  const bottomInset = scrollPadding(container, 'Bottom') || 12
  const elRect = el.getBoundingClientRect()
  const containerRect = container.getBoundingClientRect()
  // 在扣除 chrome / 底边 inset 后的可视区内垂直居中
  const visibleHeight = containerRect.height - topInset - bottomInset
  const offset = elRect.top - (containerRect.top + topInset) + elRect.height / 2 - visibleHeight / 2
  container.scrollTo({ top: container.scrollTop + offset, behavior: 'smooth' })
}

function getGroupValue(item: T): string {
  if (!props.groupField) return ''
  return String(
    typeof props.groupField === 'function' ? props.groupField(item) : item[props.groupField],
  )
}
</script>

<style scoped>
/* 列表容器 containment：限制子项 invalidation 传播范围，减少搜索结果替换时的
   WebKit 重排/重绘面积。LIMITS 已收紧（单组≤12、file≤20，全文≈44 节点），
   全量渲染开销可控，不使用 content-visibility（快速滚动有加载延迟 + 滚动条跳动）。 */
[role='listbox'] {
  contain: layout style;
}
</style>
