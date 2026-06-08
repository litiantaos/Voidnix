<template>
  <div flex gap="1.5" h="7" items="center" :title="title">
    <input
      :value="modelValue"
      type="range"
      :min="min"
      :max="max"
      :step="step"
      class="base-slider"
      :style="{ width: width }"
      @input="onInput"
    />
    <span
      v-if="!hideValue"
      text="xs tx-secondary right"
      leading="none"
      shrink="0"
      tabular-nums
      :style="{ width: valueWidth }"
      >{{ displayValue }}</span
    >
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  modelValue: number
  min?: number
  max?: number
  step?: number
  title?: string
  hideValue?: boolean
  width?: string
  valueWidth?: string
  suffix?: string
}

const props = withDefaults(defineProps<Props>(), {
  min: 0,
  max: 100,
  step: 1,
  hideValue: false,
  width: '64px',
  valueWidth: '20px',
  suffix: '',
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void
}>()

function onInput(e: Event) {
  emit('update:modelValue', Number((e.target as HTMLInputElement).value))
}

const displayValue = computed(() => `${props.modelValue}${props.suffix}`)
</script>

<style scoped>
.base-slider {
  appearance: none;
  height: 4px;
  background: rgba(0, 0, 0, 0.12);
  border-radius: 9999px;
  outline: none;
}
.base-slider::-webkit-slider-thumb {
  appearance: none;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--color-accent, #3b82f6);
  border: 2px solid var(--color-surface, #fcfcfc);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
}
</style>
