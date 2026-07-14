<template>
  <div
    ref="selectRef"
    :id="id"
    data-settings-control
    :class="[
      // w-fit：默认贴合当前 label；max-w-full 防撑破父级；消费者可用 max-w-* 设上限（勿用固定 w-*）
      'ui-ctrl custom-select inline-flex w-fit max-w-full flex-none items-center relative',
      disabled ? 'ui-disabled' : '',
    ]"
    tabindex="0"
    role="combobox"
    :aria-expanded="isOpen"
    aria-haspopup="listbox"
    @keydown="onKeyDown"
    @click="toggleOpen"
  >
    <span :class="selectedLabel ? 'text-primary' : 'text-muted'" class="min-w-0 truncate">
      {{ selectedLabel || placeholder }}
    </span>
    <i
      class="i-ri-arrow-down-s-line"
      text="sm muted"
      ml="2"
      flex="none"
      transition="transform"
      duration="200"
      :class="isOpen ? 'rotate-180' : ''"
    />

    <Teleport to="body">
      <Transition
        enter-active-class="transition duration-150 ease-out"
        :enter-from-class="
          dropUp ? 'opacity-0 scale-95 translate-y-1' : 'opacity-0 scale-95 -translate-y-1'
        "
        enter-to-class="opacity-100 scale-100 translate-0"
        leave-active-class="transition duration-100 ease-in"
        leave-from-class="opacity-100 scale-100 translate-0"
        :leave-to-class="
          dropUp ? 'opacity-0 scale-95 translate-y-1' : 'opacity-0 scale-95 -translate-y-1'
        "
      >
        <div
          v-if="isOpen"
          ref="dropdownRef"
          data-select-dropdown
          :style="floatingStyles"
          class="dropdown-panel"
          max-w="[80vw]"
          role="listbox"
        >
          <BaseDropdownItems
            :items="panelItems"
            :active-index="highlightedIndex"
            @select="selectOption"
            @hover="(i: number) => (highlightedIndex = i)"
          />
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { useFloating } from '@/composables/useFloating'
import BaseDropdownItems, { type PanelItem } from './BaseDropdownItems.vue'

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
  'update:modelValue': [value: string | number]
}>()

const isOpen = ref(false)
const selectRef = ref<HTMLElement | null>(null)
const dropdownRef = ref<HTMLElement | null>(null)
const highlightedIndex = ref(0)
const { floatingStyles, dropUp } = useFloating(selectRef, dropdownRef, {
  isOpen,
  placement: 'bottom-start',
  offset: 4,
  padding: 12,
  matchWidth: true,
})

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
  flatItems.value.map((item, index) => (item.type === 'option' ? index : -1)).filter((i) => i >= 0),
)

const panelItems = computed<PanelItem[]>(() =>
  flatItems.value.map((f) =>
    f.type === 'group'
      ? { type: 'header', label: f.label }
      : { type: 'item', key: f.value, label: f.label },
  ),
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

/** 关闭下拉并失焦：交还 ↑↓/Enter 给 BaseList（否则 settings-control 聚焦时列表让出键盘） */
function closeAndReleaseFocus() {
  isOpen.value = false
  selectRef.value?.blur()
}

const toggleOpen = () => {
  if (props.disabled) return
  if (isOpen.value) {
    closeAndReleaseFocus()
    return
  }
  isOpen.value = true
  const currentFlatIndex = flatItems.value.findIndex(
    (item) => item.type === 'option' && item.value === props.modelValue,
  )
  highlightedIndex.value = currentFlatIndex >= 0 ? currentFlatIndex : (optionIndices.value[0] ?? 0)
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
  closeAndReleaseFocus()
}

/**
 * 键盘约定：
 * - 关闭：Enter / 空格 / ↓ 展开
 * - 展开：↑↓ 移动，Enter/空格 选中，Esc 关闭
 * - 关闭后一律失焦，↑↓ 回列表导航；再 Enter 由 BaseList execute → focus+click 打开
 */
const onKeyDown = (e: KeyboardEvent) => {
  if (props.disabled) return

  if (!isOpen.value) {
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
      e.preventDefault()
      e.stopPropagation()
      toggleOpen()
    }
    return
  }

  e.preventDefault()
  e.stopPropagation()

  if (e.key === 'Escape') {
    closeAndReleaseFocus()
  } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    const currentOptionIndex = optionIndices.value.indexOf(highlightedIndex.value)
    let nextOptionIndex: number
    if (e.key === 'ArrowDown') {
      nextOptionIndex = (currentOptionIndex + 1) % optionIndices.value.length
    } else {
      nextOptionIndex =
        (currentOptionIndex - 1 + optionIndices.value.length) % optionIndices.value.length
    }
    highlightedIndex.value = optionIndices.value[nextOptionIndex]
  } else if (e.key === 'Enter' || e.key === ' ') {
    selectOption(highlightedIndex.value)
  }
}

const onClickOutside = (e: MouseEvent) => {
  // H8：多实例并存时全局 querySelector 只返回首个匹配，导致误关；
  // 改用组件本地 dropdownRef 精确判定当前实例的下拉是否被点击
  if (isOpen.value && selectRef.value && !selectRef.value.contains(e.target as Node)) {
    if (!dropdownRef.value?.contains(e.target as Node)) {
      closeAndReleaseFocus()
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
