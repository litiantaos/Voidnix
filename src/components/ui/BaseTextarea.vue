<template>
  <div data-settings-control tabindex="0" :class="rootClass" @click="focus()" @focus="focus()">
    <textarea
      ref="textareaRef"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :rows="rows"
      class="input-base placeholder:text-muted"
      text="primary"
      p="x-3 y-2"
      resize="none"
      :class="autoResize && maxHeight <= 0 ? 'overflow-y-hidden' : 'overflow-y-auto'"
      :style="{
        maxHeight: maxHeight > 0 ? maxHeight + 'px' : undefined,
        height: autoResize ? height : undefined,
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
import { ref, computed, watch, nextTick, toRef, onMounted, onActivated } from 'vue'
import { useInputControl } from '@/composables/useInputControl'

interface Props {
  modelValue?: string
  placeholder?: string
  disabled?: boolean
  error?: boolean
  rows?: number
  maxHeight?: number
  submitOnEnter?: boolean
  /** 是否随内容自动撑高（默认 true）。false 时由 rows 决定高度，超出滚动。 */
  autoResize?: boolean
  /** 无装饰变体：去除控件背景/圆角/ring，融入所在面板（配合外部分割线布局）。 */
  plain?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  placeholder: '',
  disabled: false,
  error: false,
  rows: 3,
  maxHeight: 120,
  submitOnEnter: true,
  autoResize: true,
  plain: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  submit: []
  focus: [FocusEvent]
  blur: [FocusEvent]
  keydown: [KeyboardEvent]
  compositionstart: []
  compositionend: []
}>()

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

const rootClass = computed(() =>
  props.plain
    ? [
        'flex items-start gap-2 text-sm',
        props.error ? 'border-red-400' : '',
        props.disabled ? 'ui-disabled' : '',
      ]
    : [
        'ui-ctrl h-auto! text-sm! flex items-start gap-2 !px-0',
        props.error ? 'border-red-400' : '',
        props.disabled ? 'ui-disabled bg-black/2' : '',
      ],
)

function onInput(e: Event) {
  baseOnInput(e)
  if (props.autoResize) nextTick(() => growToFit())
}

function onCompositionStart() {
  emit('compositionstart')
}

function onCompositionEnd() {
  emit('compositionend')
}

function onKeydown(e: KeyboardEvent) {
  baseOnKeydown(e)
  if (e.isComposing || e.keyCode === 229) return
  if (e.key === 'Enter' && !e.shiftKey && props.submitOnEnter) {
    e.preventDefault()
    emit('submit')
  }
}

function growToFit() {
  const el = textareaRef.value
  if (!el || !el.isConnected) return
  height.value = 'auto'
  nextTick(() => {
    const h = el.scrollHeight
    height.value = (props.maxHeight > 0 ? Math.min(h, props.maxHeight) : h) + 'px'
  })
}

watch(
  () => props.modelValue,
  () => {
    if (props.autoResize) nextTick(() => growToFit())
  },
)

onMounted(() => {
  if (props.autoResize) nextTick(() => growToFit())
})

onActivated(() => {
  if (props.autoResize) nextTick(() => growToFit())
})

defineExpose({ focus, blur, textareaRef })
</script>
