<template>
  <div
    data-settings-control
    :class="[
      'ui-ctrl flex items-center gap-2',
      error ? 'border-red-400' : '',
      disabled ? 'ui-disabled bg-black/2' : '',
    ]"
  >
    <slot name="prefix" />
    <input
      ref="inputRef"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      class="text-tx-primary outline-none bg-transparent flex-1 min-w-0 placeholder:text-tx-hint"
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
import { useAppStore } from '@/stores/app'

interface Props {
  modelValue?: string
  placeholder?: string
  type?: 'text' | 'password' | 'email' | 'number'
  disabled?: boolean
  error?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  placeholder: '',
  type: 'text',
  disabled: false,
  error: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  focus: [FocusEvent]
  blur: [FocusEvent]
  keydown: [KeyboardEvent]
}>()

const appStore = useAppStore()

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
  appStore.setComposing(true)
}

function onCompositionEnd() {
  appStore.setComposing(false)
}

defineExpose({ focus, blur, inputRef })
</script>
