<template>
  <div p="2" flex="~ col" gap="1" role="listbox" aria-label="搜索结果">
    <template v-for="(item, i) in items" :key="i">
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
        rounded="lg"
        :class="{ 'ui-active': isItemSelected(i) }"
        @click="onItemClick(i, $event)"
        @dblclick="onItemDblClick(i)"
      >
        <slot name="item" :item="item" :index="i" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts" generic="T">
import { ref, watch, nextTick, onActivated, onDeactivated } from 'vue'
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
    /** 是否激活键盘导航（由父组件根据模块状态控制） */
    keyboardActive?: boolean
    /** IME 输入法合成状态（由父组件传入） */
    composing?: boolean
    /** ArrowUp/Down 在自定义输入框聚焦时是否仍导航（如翻译框）。
     *  全局搜索框由 data-list-execute 属性统一放行，无需此 prop。Enter 一律让出
     *  除非控件标记 data-list-execute。默认 false 保护设置界面 input 编辑。 */
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

defineExpose({ selectedIndex: localIndex, setSelectedIndex, reveal })

// ── Refs ──
const itemRefs = ref<HTMLElement[]>([])
function setItemRef(el: unknown, index: number) {
  if (el) {
    itemRefs.value[index] = (el as { $el: HTMLElement }).$el || (el as HTMLElement)
  }
}

// ── Multi-select ──
let anchorIndex = -1

function getId(item: T): string {
  return (item as Record<string, unknown>)[props.idField] as string
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

// ── Keyboard 守卫 ──

/// 公共守卫：未激活 / IME 合成中不响应
function canNavigate(e: KeyboardEvent): boolean {
  if (!isActive.value || !props.keyboardActive) return false
  if (props.composing || isComposingCheck(e)) return false
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

  // 有多选项时 ESC 先清选择（不退出模块）：子组件 onMounted 先于父，listener 注册在
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

  const PADDING = 8
  const elRectTop = topElement.getBoundingClientRect().top
  const elRectBottom = el.getBoundingClientRect().bottom
  const containerRect = container.getBoundingClientRect()

  if (elRectBottom > containerRect.bottom - PADDING) {
    container.scrollTop += elRectBottom - containerRect.bottom + PADDING
  } else if (elRectTop < containerRect.top + PADDING) {
    container.scrollTop -= containerRect.top - elRectTop + PADDING
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
  const elRect = el.getBoundingClientRect()
  const containerRect = container.getBoundingClientRect()
  const offset = elRect.top - containerRect.top + elRect.height / 2 - containerRect.height / 2
  container.scrollTo({ top: container.scrollTop + offset, behavior: 'smooth' })
}

function getGroupValue(item: T): string {
  if (!props.groupField) return ''
  return String(
    typeof props.groupField === 'function' ? props.groupField(item) : item[props.groupField],
  )
}
</script>
