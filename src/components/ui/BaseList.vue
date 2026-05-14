<script setup lang="ts" generic="T">
import { ref, watch, nextTick } from 'vue'
import { onKeyStroke } from '@vueuse/core'
import { useAppStore } from '@/stores/app'

const appStore = useAppStore()

const props = withDefaults(
  defineProps<{
    items: T[]
    selectedIndex?: number
    keyboardNavigation?: boolean
    groupField?: keyof T | ((item: T) => string)
    groupTitle?: (group: string) => string
  }>(),
  {
    selectedIndex: 0,
    keyboardNavigation: false,
  },
)

const emit = defineEmits<{
  'update:selectedIndex': [index: number]
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

const itemRefs = ref<HTMLElement[]>([])
function setItemRef(el: unknown, index: number) {
  if (el) {
    // If it's a Vue component, use $el, otherwise use the element itself
    itemRefs.value[index] =
      (el as { $el: HTMLElement }).$el || (el as HTMLElement)
  }
}

const isKeyboardNavigation = ref(false)
let lastMousePos = { x: 0, y: 0 }

function onMouseMove(e: MouseEvent) {
  if (
    Math.abs(e.clientX - lastMousePos.x) > 1 ||
    Math.abs(e.clientY - lastMousePos.y) > 1
  ) {
    isKeyboardNavigation.value = false
  }
  lastMousePos = { x: e.clientX, y: e.clientY }
}

watch(localIndex, async (index) => {
  isKeyboardNavigation.value = true
  await nextTick()

  const el = itemRefs.value[index]
  if (el) {
    const container = el.closest('.overflow-y-auto, .overflow-auto')
    if (container) {
      const itemWrapper = el.parentElement!
      let topElement: HTMLElement = itemWrapper

      let isFirstInGroup = index === 0
      if (!isFirstInGroup && props.groupField) {
        const currentGroup = getGroupValue(props.items[index])
        const prevGroup = getGroupValue(props.items[index - 1])
        if (currentGroup !== prevGroup) {
          isFirstInGroup = true
        }
      }

      if (isFirstInGroup && itemWrapper.previousElementSibling) {
        topElement = itemWrapper.previousElementSibling as HTMLElement
      }

      const elRectTop = topElement.getBoundingClientRect().top
      const elRectBottom = itemWrapper.getBoundingClientRect().bottom
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

// Enable keyboard navigation if requested
if (props.keyboardNavigation) {
  function isSettingsControl(activeEl: Element | null) {
    return (
      activeEl &&
      (['INPUT', 'TEXTAREA', 'SELECT'].includes(activeEl.tagName) ||
        activeEl.classList.contains('custom-select') ||
        activeEl.hasAttribute('data-settings-control'))
    )
  }

  onKeyStroke('ArrowDown', (e) => {
    if (!appStore.activeModuleId) return
    if (appStore.isComposing || e.isComposing || e.keyCode === 229) return
    const activeEl = document.activeElement
    if (isSettingsControl(activeEl) && activeEl!.id !== 'main-search-input') return
    e.preventDefault()
    if (props.items.length > 0) {
      setSelectedIndex(
        localIndex.value >= props.items.length - 1 ? 0 : localIndex.value + 1,
      )
    }
  })
  onKeyStroke('ArrowUp', (e) => {
    if (!appStore.activeModuleId) return
    if (appStore.isComposing || e.isComposing || e.keyCode === 229) return
    const activeEl = document.activeElement
    if (isSettingsControl(activeEl) && activeEl!.id !== 'main-search-input') return
    e.preventDefault()
    if (props.items.length > 0) {
      setSelectedIndex(
        localIndex.value <= 0 ? props.items.length - 1 : localIndex.value - 1,
      )
    }
  })
  onKeyStroke('Enter', (e) => {
    if (!appStore.activeModuleId) return
    if (appStore.isComposing || e.isComposing || e.keyCode === 229) return
    const activeEl = document.activeElement
    if (isSettingsControl(activeEl) && activeEl!.id !== 'main-search-input') return
    e.preventDefault()
    if (props.items.length > 0) {
      emit('execute', props.items[localIndex.value], localIndex.value, e)
    }
  })
}

function getGroupValue(item: T): string {
  if (!props.groupField) return ''
  return String(
    typeof props.groupField === 'function'
      ? props.groupField(item)
      : item[props.groupField],
  )
}
</script>

<template>
  <div @mousemove="onMouseMove" class="p-2 flex flex-col gap-1">
    <template v-for="(item, i) in items" :key="i">
      <div
        v-if="
          groupField &&
          (i === 0 || getGroupValue(item) !== getGroupValue(items[i - 1])) &&
          (groupTitle ? groupTitle(getGroupValue(item)) : getGroupValue(item))
        "
        class="text-xs text-tx-faint tracking-wider font-medium px-3 py-1.5 uppercase"
      >
        <slot
          name="group-title"
          :group="getGroupValue(item)"
          :item="item"
          :index="i"
        >
          {{
            groupTitle ? groupTitle(getGroupValue(item)) : getGroupValue(item)
          }}
        </slot>
      </div>

      <div>
        <slot
          name="item"
          :item="item"
          :index="i"
          :selected="localIndex === i"
          :hoverable="!isKeyboardNavigation"
          :set-ref="(el: unknown) => setItemRef(el, i)"
          :select="() => setSelectedIndex(i)"
          :execute="() => emit('execute', item, i)"
        />
      </div>
    </template>
  </div>
</template>
