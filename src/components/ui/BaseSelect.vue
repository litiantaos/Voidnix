<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'

interface Option {
  label: string
  value: string | number
}

interface OptionGroup {
  label: string
  options: Option[]
}

function isOptionGroup(item: Option | OptionGroup): item is OptionGroup {
  return 'options' in item
}

interface Props {
  id?: string
  modelValue: string | number
  options: (Option | OptionGroup)[]
  placeholder?: string
  disabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  placeholder: '请选择',
  disabled: false,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: string | number): void
}>()

const isOpen = ref(false)
const selectRef = ref<HTMLElement | null>(null)
const highlightedIndex = ref(0)
const dropUp = ref(false)
const dropdownStyle = ref<Record<string, string>>({})

interface FlatGroupItem {
  type: 'group'
  label: string
}

interface FlatOptionItem {
  type: 'option'
  label: string
  value: string | number
}

type FlatItem = FlatGroupItem | FlatOptionItem

const flatItems = computed<FlatItem[]>(() => {
  const result: FlatItem[] = []
  for (const item of props.options) {
    if (isOptionGroup(item)) {
      if (item.options.length === 0) continue
      result.push({ type: 'group', label: item.label })
      for (const opt of item.options) {
        result.push({ type: 'option', ...opt })
      }
    } else {
      result.push({ type: 'option', ...item })
    }
  }
  return result
})

const optionIndices = computed(() =>
  flatItems.value
    .map((item, index) => (item.type === 'option' ? index : -1))
    .filter((i) => i >= 0),
)

const selectedLabel = computed(() => {
  for (const item of props.options) {
    if (isOptionGroup(item)) {
      const opt = item.options.find((o) => o.value === props.modelValue)
      if (opt) return opt.label
    } else {
      if (item.value === props.modelValue) return item.label
    }
  }
  return ''
})

const OPTION_HEIGHT = 32
const GROUP_HEIGHT = 28
const PADDING = 8
const GAP = 4
const MARGIN = 20

const calcDropdownHeight = () => {
  let height = 0
  for (const item of flatItems.value) {
    height += item.type === 'group' ? GROUP_HEIGHT : OPTION_HEIGHT
  }
  return height + PADDING
}

const calcDropdownStyle = () => {
  if (!selectRef.value) return
  const rect = selectRef.value.getBoundingClientRect()
  const windowHeight = window.innerHeight
  const windowWidth = window.innerWidth
  const dropdownHeight = calcDropdownHeight()
  const spaceBelow = windowHeight - rect.bottom - GAP
  const spaceAbove = rect.top - GAP

  let top: number
  let maxHeight: number | undefined

  if (spaceBelow >= dropdownHeight) {
    dropUp.value = false
    top = rect.bottom + GAP
    maxHeight = undefined
  } else if (spaceAbove >= dropdownHeight) {
    dropUp.value = true
    top = rect.top - GAP - dropdownHeight
    maxHeight = undefined
  } else if (spaceAbove > spaceBelow) {
    dropUp.value = true
    maxHeight = Math.floor(spaceAbove / OPTION_HEIGHT) * OPTION_HEIGHT + PADDING
    top = rect.top - GAP - maxHeight
  } else {
    dropUp.value = false
    maxHeight = Math.floor(spaceBelow / OPTION_HEIGHT) * OPTION_HEIGHT + PADDING
    top = rect.bottom + GAP
  }

  const maxDropdownWidth = windowWidth - 2 * MARGIN
  const minDropdownWidth = rect.width

  const leftAlignFits = rect.left + maxDropdownWidth <= windowWidth
  const rightAlignFits = rect.right - maxDropdownWidth >= 0

  let horizontalStyle: Record<string, string>
  if (leftAlignFits) {
    horizontalStyle = { left: `${rect.left}px` }
  } else if (rightAlignFits) {
    horizontalStyle = { right: `${windowWidth - rect.right}px` }
  } else {
    horizontalStyle =
      rect.left + rect.width / 2 < windowWidth / 2
        ? { left: `${MARGIN}px` }
        : { right: `${MARGIN}px` }
  }

  dropdownStyle.value = {
    position: 'fixed',
    top: `${top}px`,
    minWidth: `${minDropdownWidth}px`,
    maxWidth: `${maxDropdownWidth}px`,
    zIndex: '9999',
    ...horizontalStyle,
    ...(maxHeight !== undefined
      ? { maxHeight: `${maxHeight}px`, overflowY: 'auto' }
      : {}),
  }
}

const toggleOpen = () => {
  if (props.disabled) return
  isOpen.value = !isOpen.value
  if (isOpen.value) {
    const currentFlatIndex = flatItems.value.findIndex(
      (item) => item.type === 'option' && item.value === props.modelValue,
    )
    highlightedIndex.value =
      currentFlatIndex >= 0
        ? currentFlatIndex
        : optionIndices.value[0] ?? 0
    calcDropdownStyle()
  }
}

const focus = () => {
  selectRef.value?.focus()
}

const blur = () => {
  selectRef.value?.blur()
}

defineExpose({ toggleOpen, focus, blur })

const selectOption = (index: number) => {
  const item = flatItems.value[index]
  if (!item || item.type !== 'option') return
  emit('update:modelValue', item.value)
  isOpen.value = false
  selectRef.value?.focus()
  selectRef.value?.blur()
}

const onKeyDown = (e: KeyboardEvent) => {
  if (props.disabled) return

  if (!isOpen.value) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      toggleOpen()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      toggleOpen()
    }
    return
  }

  e.preventDefault()
  e.stopPropagation()

  if (e.key === 'Escape') {
    isOpen.value = false
    selectRef.value?.blur()
  } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    const currentOptionIndex = optionIndices.value.indexOf(highlightedIndex.value)
    let nextOptionIndex: number
    if (e.key === 'ArrowDown') {
      nextOptionIndex = (currentOptionIndex + 1) % optionIndices.value.length
    } else {
      nextOptionIndex =
        (currentOptionIndex - 1 + optionIndices.value.length) %
        optionIndices.value.length
    }
    highlightedIndex.value = optionIndices.value[nextOptionIndex]
  } else if (e.key === 'Enter' || e.key === ' ') {
    selectOption(highlightedIndex.value)
  }
}

const onClickOutside = (e: MouseEvent) => {
  if (
    isOpen.value &&
    selectRef.value &&
    !selectRef.value.contains(e.target as Node)
  ) {
    const dropdown = document.querySelector('[data-select-dropdown]')
    if (!dropdown?.contains(e.target as Node)) {
      isOpen.value = false
    }
  }
}

onMounted(() => {
  document.addEventListener('mousedown', onClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onClickOutside)
})
</script>

<template>
  <div
    ref="selectRef"
    :id="id"
    data-settings-control
    :class="[
      'ui-ctrl custom-select flex items-center justify-between ui-clickable min-w-0 w-full relative overflow-hidden',
      disabled ? 'ui-disabled' : '',
    ]"
    tabindex="0"
    @keydown="onKeyDown"
    @click="toggleOpen"
  >
    <span
      :class="selectedLabel ? 'text-tx-primary' : 'text-tx-hint'"
      class="truncate"
    >
      {{ selectedLabel || placeholder }}
    </span>
    <i
      class="i-ri-arrow-down-s-line text-sm text-tx-muted ml-2 flex-none transition-transform duration-200"
      :class="isOpen ? 'rotate-180' : ''"
    />

    <Teleport to="body">
      <Transition
        enter-active-class="transition duration-150 ease-out"
        :enter-from-class="
          dropUp
            ? 'opacity-0 scale-95 translate-y-1'
            : 'opacity-0 scale-95 -translate-y-1'
        "
        enter-to-class="opacity-100 scale-100 translate-0"
        leave-active-class="transition duration-100 ease-in"
        leave-from-class="opacity-100 scale-100 translate-0"
        :leave-to-class="
          dropUp
            ? 'opacity-0 scale-95 translate-y-1'
            : 'opacity-0 scale-95 -translate-y-1'
        "
      >
        <div
          v-if="isOpen"
          data-select-dropdown
          :style="dropdownStyle"
          class="p-1 rounded-lg bg-white max-w-[80vw] shadow-lg"
        >
          <template v-for="(item, index) in flatItems" :key="index">
            <div
              v-if="item.type === 'group'"
              class="text-xs text-tx-faint tracking-wider font-medium px-3 py-1.5 uppercase"
            >
              {{ item.label }}
            </div>
            <div
              v-else
              @click.stop="selectOption(index)"
              @mouseover="highlightedIndex = index"
              :class="[
                'text-sm font-medium px-3 py-1.5 rounded-md cursor-pointer transition-colors truncate',
                index === highlightedIndex
                  ? 'ui-active text-accent'
                  : 'ui-hover text-tx-secondary',
              ]"
            >
              {{ item.label }}
            </div>
          </template>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
