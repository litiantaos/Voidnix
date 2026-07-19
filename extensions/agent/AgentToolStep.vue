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
      <span class="agent-step-label" shrink="0" font="medium">{{ toolLabel(part.name) }}</span>
      <span v-if="detail" shrink-1 min-w="0" font="mono" text="muted" truncate>{{ detail }}</span>
      <span
        v-if="part.state === 'streaming' || part.state === 'running'"
        class="agent-step-dots"
        aria-hidden="true"
      >
        <i /><i /><i />
      </span>
    </div>
    <!-- web_search 成功 hits；失败 / 其他工具：output 原文 -->
    <div v-if="showBody" class="agent-step-body" flex gap="1.5" min-w="0">
      <div class="agent-step-gutter" shrink="0" aria-hidden="true">
        <span class="agent-step-rail" />
      </div>
      <div
        v-if="part.parsed && part.state === 'done'"
        class="agent-step-out"
        flex="~ col"
        gap="1"
        min-w="0"
      >
        <p v-if="part.parsed.answer" text="secondary" leading="snug">
          {{ part.parsed.answer }}
        </p>
        <button
          v-for="(hit, hi) in part.parsed.hits"
          :key="hi"
          type="button"
          class="agent-hit"
          :title="hit.url"
          @click="emit('open-url', hit.url)"
        >
          <div flex items="center" gap="1" min-w="0">
            <span class="agent-hit-title" text="primary" truncate>
              {{ hit.title || hit.url }}
            </span>
            <i shrink="0" class="i-ri-external-link-line" text="muted" aria-hidden="true" />
          </div>
          <div text="muted" truncate>{{ hit.snippet || hit.url }}</div>
        </button>
      </div>
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

const emit = defineEmits<{
  'open-url': [url: string]
}>()

const detail = computed(() => toolDetail(props.part))
const showBody = computed(() => showToolBody(props.part))
</script>

<style scoped>
.agent-hit {
  display: block;
  width: 100%;
  min-width: 0;
  margin: 0;
  padding: 2px 0;
  border: 1px solid transparent;
  background: transparent;
  cursor: pointer;
  font: inherit;
  color: inherit;
  text-align: left;
  border-radius: var(--radius-ctrl);
}

.agent-hit:hover .agent-hit-title {
  text-decoration: underline;
}

.agent-hit:focus-visible {
  outline: none;
  border-color: var(--focus-ring-color);
}
</style>
