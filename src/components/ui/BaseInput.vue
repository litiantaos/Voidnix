<template>
  <div
    :class="[
      // 默认 soft-chip；panel = soft-surface 白边面（非 ui-field；大输入用 BaseTextarea）
      // ui-input：走 focus-within 聚焦环 + 抑制 soft-chip:active
      'ui-ctrl ui-input flex items-center gap-2',
      rounded === 'panel' ? 'soft-surface !radius-panel' : 'soft-chip',
      error ? 'border border-danger' : '',
      disabled ? 'ui-disabled' : '',
      // 有 suffix（密码眼睛等）时右内边距收紧，图标更贴右
      hasSuffix ? '!pr-1' : '',
    ]"
    @click="focus()"
  >
    <slot name="prefix" />
    <input
      ref="inputRef"
      data-settings-control
      :tabindex="disabled ? -1 : 0"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      class="input-base placeholder:text-muted"
      text="primary"
      @input="onInput"
      @focus="onFocus"
      @blur="emit('blur', $event)"
      @keydown="onKeydownHandler"
      @compositionstart="onCompositionStart"
      @compositionend="onCompositionEnd"
    />
    <slot name="suffix" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, toRef, useSlots } from 'vue'
import { useInputControl } from '@/composables/useInputControl'

interface Props {
  modelValue?: string
  placeholder?: string
  type?: 'text' | 'password' | 'email' | 'number'
  disabled?: boolean
  error?: boolean
  /** 圆角档：ctrl（默认 6）/ panel（10，主输入面） */
  rounded?: 'ctrl' | 'panel'
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  placeholder: '',
  type: 'text',
  disabled: false,
  error: false,
  rounded: 'ctrl',
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  focus: [FocusEvent]
  blur: [FocusEvent]
  keydown: [KeyboardEvent]
  compositionstart: []
  compositionend: []
}>()

const slots = useSlots()
const hasSuffix = computed(() => !!slots.suffix)

const {
  elRef: inputRef,
  onInput,
  onKeydown,
  focus,
  blur,
} = useInputControl<HTMLInputElement>({
  modelValue: toRef(props, 'modelValue'),
  emit,
})

/** 聚焦前的值，供 Esc 还原。 */
const valueBeforeEdit = ref('')

function onFocus(e: FocusEvent) {
  valueBeforeEdit.value = props.modelValue
  emit('focus', e)
}

/**
 * Enter：失焦提交（值已通过 @input 同步）。
 * Escape：模态弹窗内冒泡（弹窗自行关闭）；否则还原值 + 失焦。
 */
function onKeydownHandler(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    e.stopPropagation()
    blur()
    return
  }
  if (e.key === 'Escape') {
    if (document.querySelector('[role="dialog"][aria-modal="true"]')) return
    e.preventDefault()
    e.stopPropagation()
    emit('update:modelValue', valueBeforeEdit.value)
    blur()
    return
  }
  onKeydown(e)
}

function onCompositionStart() {
  emit('compositionstart')
}

function onCompositionEnd() {
  emit('compositionend')
}

defineExpose({ focus, blur, inputRef })
</script>
