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
      p="3"
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
  /**
   * 圆角档：ctrl（默认 6，表单内嵌）/ panel（10，与搜索栏/列表选中同级的主输入面）。
   * 翻译 / Agent 等模块主输入用 panel。
   */
  rounded?: 'ctrl' | 'panel'
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
  rounded: 'ctrl',
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

const rootClass = computed(() => [
  // 大输入走 ui-field（soft-surface 材质 + border 描边，见 theme.css），非工具条 soft-chip
  'ui-field h-auto! flex items-start gap-2 !px-0',
  props.rounded === 'panel' ? '!radius-panel' : '',
  props.error ? 'border-danger' : '',
  props.disabled ? 'ui-disabled' : '',
])

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
