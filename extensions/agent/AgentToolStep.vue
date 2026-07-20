<template>
  <div
    class="agent-step"
    :class="{
      'agent-step--active': part.state === 'streaming' || part.state === 'running',
      'agent-step--failed': part.state === 'failed',
    }"
    text="xs"
    flex="~ col"
    gap="1"
    :style="{ animationDelay: `${Math.min(index, 6) * 40}ms` }"
  >
    <div flex items="center" gap="1.5" min-w="0">
      <span shrink="0" class="agent-step-icon" :class="toolIcon(part.name)" text="accent" />
      <span class="agent-step-label" shrink="0" font="medium">{{ label }}</span>
      <span v-if="detail" shrink-1 min-w="0" font="mono" text="secondary" truncate>{{
        detail
      }}</span>
      <span
        v-if="part.state === 'streaming' || part.state === 'running'"
        class="agent-step-dots"
        aria-hidden="true"
      >
        <i /><i /><i />
      </span>
    </div>
    <!-- web_search answer 摘要；失败 / 其他工具：output 原文 -->
    <div v-if="showBody" class="agent-step-body" flex gap="1.5" min-w="0">
      <div class="agent-step-gutter" shrink="0" aria-hidden="true">
        <span class="agent-step-rail" />
      </div>
      <p v-if="answer" class="agent-step-out agent-step-clamp" text="secondary" leading="snug">
        {{ answer }}
      </p>
      <pre
        v-else-if="part.output"
        class="agent-step-out"
        font="mono"
        text="muted"
        whitespace="pre-wrap"
        break="words"
        max-h="40"
        overflow="auto"
        >{{ part.output }}</pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { AgentPart } from '@/types/agent'
import { showToolBody, toolDetail, toolIcon, toolLabel } from './view-logic'
import './agent-step.css'

const props = defineProps<{
  part: Extract<AgentPart, { type: 'toolCall' }>
  /** part 在消息内的序号，用于入场 stagger */
  index: number
}>()

const label = computed(() => toolLabel(props.part.name))
const detail = computed(() => toolDetail(props.part))
const answer = computed(() =>
  props.part.name === 'web_search' && props.part.state === 'done' ? props.part.parsed : undefined,
)
const showBody = computed(() => showToolBody(props.part))
</script>
