<template>
  <div
    data-settings-control
    tabindex="0"
    :class="[
      'ui-ctrl h-auto! text-sm! flex items-start gap-2',
      error ? 'border-red-400' : '',
      disabled ? 'ui-disabled bg-black/2' : '',
    ]"
    @click="focus()"
  >
    <textarea
      ref="textareaRef"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :rows="rows"
      class="text-tx-primary py-2 outline-none bg-transparent flex-1 min-w-0 resize-none overflow-y-hidden placeholder:text-tx-hint"
      :class="{ 'overflow-y-auto': maxHeight > 0 }"
      :style="{
        maxHeight: maxHeight > 0 ? maxHeight + 'px' : undefined,
        height,
      }"
      @input="onInput"
      @keydown="onKeydown"
      @compositionstart="onCompositionStart"
      @compositionend="onCompositionEnd"
      @focus="emit('focus', $event)"
      @blur="emit('blur', $event)"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, toRef, onMounted, onActivated } from 'vue'
import { useInputControl } from '@/composables/useInputControl'
import { useAppStore } from '@/stores/app'

interface Props {
  modelValue?: string
  placeholder?: string
  disabled?: boolean
  error?: boolean
  rows?: number
  maxHeight?: number
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  placeholder: '',
  disabled: false,
  error: false,
  rows: 3,
  maxHeight: 120,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  submit: []
  focus: [FocusEvent]
  blur: [FocusEvent]
  keydown: [KeyboardEvent]
}>()

const appStore = useAppStore()

const {
  elRef: textareaRef,
  onInput: baseOnInput,
  onKeydown: baseOnKeydown,
  focus,
  blur,
} = useInputControl<HTMLTextAreaElement>({
  modelValue: toRef(props, 'modelValue'),
  emit,
})

const height = ref<string>()

function onInput(e: Event) {
  baseOnInput(e)
  nextTick(() => autoResize())
}

function onCompositionStart() {
  appStore.setComposing(true)
}

function onCompositionEnd() {
  appStore.setComposing(false)
}

function onKeydown(e: KeyboardEvent) {
  baseOnKeydown(e)
  if (appStore.isComposing || e.isComposing || e.keyCode === 229) return
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    emit('submit')
  }
}

function autoResize() {
  const el = textareaRef.value
  if (!el || !el.isConnected) return
  el.style.height = 'auto'
  const h = el.scrollHeight
  height.value = (props.maxHeight > 0 ? Math.min(h, props.maxHeight) : h) + 'px'
}

watch(() => props.modelValue, () => {
  nextTick(() => autoResize())
})

onMounted(() => {
  nextTick(() => autoResize())
})

onActivated(() => {
  nextTick(() => autoResize())
})

defineExpose({ focus, blur, textareaRef })
</script>
