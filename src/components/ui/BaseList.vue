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
        @click="onItemClick(i, $event)"
        @dblclick="onItemDblClick(i)"
      >
        <slot
          name="item"
          :item="item"
          :index="i"
          :selected="localIndex === i"
          :multi-selected="isMultiSelected(i)"
          :set-ref="undefined"
          :select="() => setSelectedIndex(i)"
          :execute="() => emit('execute', item, i)"
        />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts" generic="T">
import { ref, watch, nextTick, onActivated, onDeactivated } from 'vue'
import { onKeyStroke } from '@/composables/events'
import { isComposing as isComposingCheck, isFormControl, wrapIndex } from '@/utils/dom'

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
  }>(),
  {
    selectedIndex: 0,
    multiSelect: false,
    idField: 'id',
    keyboardActive: true,
    composing: false,
  },
)

const emit = defineEmits<{
  'update:selectedIndex': [index: number]
  'update:selectedIds': [ids: Set<string>]
  select: [index: number]
  execute: [item: T, index: number, event?: KeyboardEvent]
  reveal: [item: T]
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

defineExpose({ selectedIndex: localIndex, setSelectedIndex })

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

function emitIds(ids: Set<string>) {
  emit('update:selectedIds', ids)
}

function onItemClick(index: number, e: MouseEvent) {
  if (props.multiSelect && (e.metaKey || e.ctrlKey || e.shiftKey)) {
    const ids = new Set(props.selectedIds ?? [])
    if (e.shiftKey) {
      if (anchorIndex < 0) anchorIndex = localIndex.value
      const [start, end] = [Math.min(anchorIndex, index), Math.max(anchorIndex, index)]
      const newIds = new Set<string>()
      for (let i = start; i <= end; i++) {
        newIds.add(getId(props.items[i]))
      }
      setSelectedIndex(index)
      emitIds(newIds)
    } else {
      if (ids.size === 0) {
        ids.add(getId(props.items[localIndex.value]))
        if (anchorIndex < 0) anchorIndex = localIndex.value
      }
      const id = getId(props.items[index])
      if (ids.has(id)) ids.delete(id)
      else ids.add(id)
      setSelectedIndex(index)
      emitIds(ids)
    }
  } else {
    anchorIndex = index
    setSelectedIndex(index)
  }
}

function onItemDblClick(index: number) {
  emit('execute', props.items[index], index)
  if (props.multiSelect) emitIds(new Set())
}

// ── Keyboard ──
onKeyStroke(['ArrowDown', 'ArrowUp'], (e) => {
  if (!isActive.value) return
  if (!props.keyboardActive) return
  if (props.composing || isComposingCheck(e)) return
  if (
    isFormControl(document.activeElement, { settingsControl: true }) &&
    (document.activeElement as Element).id !== 'main-search-input'
  )
    return

  const direction = e.key === 'ArrowDown' ? 'down' : 'up'

  if (props.multiSelect && e.shiftKey) {
    e.preventDefault()
    if (direction === 'down') {
      const next = Math.min(localIndex.value + 1, props.items.length - 1)
      if (next !== localIndex.value) {
        if (anchorIndex < 0) anchorIndex = localIndex.value
        const [start, end] = [Math.min(anchorIndex, next), Math.max(anchorIndex, next)]
        const ids = new Set<string>()
        for (let i = start; i <= end; i++) {
          ids.add(getId(props.items[i]))
        }
        setSelectedIndex(next)
        emitIds(ids)
      }
    } else {
      const next = Math.max(localIndex.value - 1, 0)
      if (next !== localIndex.value) {
        if (anchorIndex < 0) anchorIndex = localIndex.value
        const [start, end] = [Math.min(anchorIndex, next), Math.max(anchorIndex, next)]
        const ids = new Set<string>()
        for (let i = start; i <= end; i++) {
          ids.add(getId(props.items[i]))
        }
        setSelectedIndex(next)
        emitIds(ids)
      }
    }
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
    if (!isActive.value) return
    if (!props.keyboardActive) return
    if (!(e.metaKey || e.ctrlKey)) return
    e.preventDefault()
    const ids = new Set(props.items.map((item) => getId(item)))
    emitIds(ids)
  })

  onKeyStroke('Escape', (e) => {
    if (!isActive.value) return
    if (!props.keyboardActive) return
    if ((props.selectedIds?.size ?? 0) === 0) return
    e.preventDefault()
    emitIds(new Set())
  })
}

onKeyStroke('Enter', (e) => {
  if (!isActive.value) return
  if (!props.keyboardActive) return
  if (props.composing || isComposingCheck(e)) return
  if (
    isFormControl(document.activeElement, { settingsControl: true }) &&
    (document.activeElement as Element).id !== 'main-search-input'
  )
    return
  if (
    document.activeElement?.tagName === 'BUTTON' &&
    document.activeElement!.id !== 'main-search-input'
  )
    return
  e.preventDefault()
  if (props.items.length > 0) {
    if (e.metaKey) {
      const item = props.items[localIndex.value] as Record<string, unknown>
      if ((item?.data as Record<string, unknown>)?.path) {
        emit('reveal', props.items[localIndex.value])
        return
      }
    }
    const el = itemRefs.value[localIndex.value]
    if (el) {
      const control = el.querySelector<HTMLElement>('[data-settings-control][tabindex="0"]')
      if (control) {
        control.focus()
        control.click()
        return
      }
    }
    emit('execute', props.items[localIndex.value], localIndex.value, e)
    if (props.multiSelect) emitIds(new Set())
  }
})

// ── Scroll ──
watch(localIndex, async (index) => {
  await nextTick()
  const el = itemRefs.value[index]
  if (el) {
    const container = el.closest('.overflow-y-auto, .overflow-auto')
    if (container) {
      let topElement: HTMLElement = el

      let isFirstInGroup = index === 0
      if (!isFirstInGroup && props.groupField) {
        const currentGroup = getGroupValue(props.items[index])
        const prevGroup = getGroupValue(props.items[index - 1])
        if (currentGroup !== prevGroup) {
          isFirstInGroup = true
        }
      }

      if (isFirstInGroup && el.previousElementSibling) {
        topElement = el.previousElementSibling as HTMLElement
      }

      const elRectTop = topElement.getBoundingClientRect().top
      const elRectBottom = el.getBoundingClientRect().bottom
      const containerRect = container.getBoundingClientRect()
      const PADDING = 8

      if (elRectBottom > containerRect.bottom - PADDING) {
        container.scrollTop += elRectBottom - containerRect.bottom + PADDING
      } else if (elRectTop < containerRect.top + PADDING) {
        container.scrollTop -= containerRect.top - elRectTop + PADDING
      }
    }
  }
})

function getGroupValue(item: T): string {
  if (!props.groupField) return ''
  return String(
    typeof props.groupField === 'function' ? props.groupField(item) : item[props.groupField],
  )
}
</script>
