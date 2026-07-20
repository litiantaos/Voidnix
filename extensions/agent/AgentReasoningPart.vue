<template>
  <div
    class="agent-step"
    :class="{ 'agent-step--active': streaming }"
    text="xs"
    flex="~ col"
    gap="1"
  >
    <div flex items="center" gap="1.5" min-w="0">
      <span
        shrink="0"
        class="agent-step-icon i-ri-sparkling-2-line"
        text="accent"
        aria-hidden="true"
      />
      <span class="agent-step-label" shrink="0" font="medium">思考</span>
      <span v-if="streaming" class="agent-step-dots" aria-hidden="true"> <i /><i /><i /> </span>
    </div>
    <div v-if="text" class="agent-step-body" flex gap="1.5" min-w="0">
      <div class="agent-step-gutter" shrink="0" aria-hidden="true">
        <span class="agent-step-rail" />
      </div>
      <p class="agent-step-out agent-step-clamp" text="secondary" leading="snug">{{ text }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import './agent-step.css'

defineProps<{
  text: string
  /** 是否正在流式输出（最后一个 part 且消息 streaming） */
  streaming: boolean
}>()
</script>
