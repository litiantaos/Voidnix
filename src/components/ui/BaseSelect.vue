<template>
  <div
    ref="selectRef"
    :id="id"
    data-settings-control
    :class="[
      // 控件统一 soft-chip；w-fit 贴合 label，max-w-full 防撑破父级
      'ui-ctrl soft-chip custom-select inline-flex w-fit max-w-full flex-none items-center relative',
      disabled ? 'ui-disabled' : '',
    ]"
    tabindex="0"
    role="combobox"
    :aria-expanded="isOpen"
    aria-haspopup="listbox"
    @keydown="onKeyDown"
    @focusout="onFocusOut"
    @mousedown="onMousedownToggle"
    @click="onClickToggle"
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
      <Transition :css="false" @enter="onEnter" @leave="onLeave">
        <!-- prevent：下拉项不可聚焦，mousedown 转焦致 focusout 抢先 closeDropdown，click 到达前下拉已卸载 -->
        <div
          v-if="isOpen"
          ref="dropdownRef"
          data-select-dropdown
          class="dropdown-panel"
          max-w="[80vw]"
          role="listbox"
          @mousedown.prevent
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
const { onEnter, onLeave } = useFloating(selectRef, dropdownRef, {
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

/** 仅关下拉，保持焦点（Tab 移走前 / 程序化关） */
function closeDropdown() {
  isOpen.value = false
}

/** 关闭并失焦：交还 ↑↓/Enter 给 BaseList（settings-control 聚焦时列表让出键盘） */
function closeAndReleaseFocus() {
  closeDropdown()
  selectRef.value?.blur()
}

/**
 * 焦点离开 combobox（含主窗 Tab 环 cycleFocus 切到邻钮）：关下拉。
 * 下拉 Teleport 到 body，relatedTarget 在面板内则保留（选项通常不可聚焦）。
 */
function onFocusOut(e: FocusEvent) {
  if (!isOpen.value) return
  const next = e.relatedTarget as Node | null
  if (next && (selectRef.value?.contains(next) || dropdownRef.value?.contains(next))) return
  closeDropdown()
}

const toggleOpen = () => {
  if (props.disabled) return
  if (isOpen.value) {
    closeAndReleaseFocus()
    return
  }
  // 清残留：上一次 mousedown 置位后若 click 被吞（WKWebView 程序聚焦场景）/ 拖拽移出未释放，
  // flag 会残留——设置页键盘 control.click() 无 mousedown 不重置，首次点击静默失效
  suppressClickToggle = false
  isOpen.value = true
  const currentFlatIndex = flatItems.value.findIndex(
    (item) => item.type === 'option' && item.value === props.modelValue,
  )
  highlightedIndex.value = currentFlatIndex >= 0 ? currentFlatIndex : (optionIndices.value[0] ?? 0)
}

/**
 * mousedown + click 双入口 toggle，flag 抑制同一手势 double toggle。
 *
 * 根因：WKWebView 在 textarea 经 .focus() 程序化聚焦后，首次用户点击只触发 mousedown
 * 不触发 click（click 被失焦过程吞掉），导致下拉首次点击不弹。故 toggle 下沉 mousedown；
 * click 保留兜底设置页键盘流程（BaseSettingsList Enter → control.click()，无 mousedown）。
 */
let suppressClickToggle = false

function onMousedownToggle() {
  toggleOpen()
  suppressClickToggle = true
}

function onClickToggle() {
  if (suppressClickToggle) {
    suppressClickToggle = false
    return
  }
  toggleOpen()
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
 * 键盘约定（对齐 combobox）：
 * - 关闭：Enter / 空格 / ↓ 展开
 * - 展开：↑↓ 移动，Enter/空格 选中，Esc 关闭并失焦
 * - 展开 + Tab：只关下拉、不拦截，焦点由 Tab 环 / 浏览器移走（focusout 兜底）
 * - 关闭后失焦场景：↑↓ 回列表；再 Enter 由 BaseList execute → focus+click 打开
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

  // Tab：关列表并放行（主窗 capture Tab 环会 cycleFocus；设置页走浏览器默认）
  if (e.key === 'Tab') {
    closeDropdown()
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
  // 多实例：用本地 dropdownRef，忌全局 querySelector
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
