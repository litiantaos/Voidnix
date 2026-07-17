<template>
  <div
    data-settings-control
    tabindex="0"
    :class="[
      // 默认 soft-chip；panel = soft-surface 白边面（非 ui-field；大输入用 BaseTextarea）
      'ui-ctrl flex items-center gap-2',
      rounded === 'panel' ? 'soft-surface !radius-panel' : 'soft-chip',
      error ? 'border border-danger' : '',
      disabled ? 'ui-disabled' : '',
    ]"
    @click="focus()"
    @focus="focus()"
  >
    <slot name="prefix" />
    <input
      ref="inputRef"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      class="input-base placeholder:text-muted"
      text="primary"
      @input="onInput"
      @focus="emit('focus', $event)"
      @blur="emit('blur', $event)"
      @keydown="onKeydown"
      @compositionstart="onCompositionStart"
      @compositionend="onCompositionEnd"
    />
    <slot name="suffix" />
  </div>
</template>

<script setup lang="ts">
import { toRef } from 'vue'
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

function onCompositionStart() {
  emit('compositionstart')
}

function onCompositionEnd() {
  emit('compositionend')
}

defineExpose({ focus, blur, inputRef })
</script>
