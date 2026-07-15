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
        <div
          v-for="(hit, hi) in part.parsed.hits"
          :key="hi"
          py="0.5"
          cursor="pointer"
          @click="emit('open-url', hit.url)"
        >
          <div text="primary" truncate class="hover:underline">
            {{ hit.title || hit.url }}
          </div>
          <div text="muted" truncate>{{ hit.snippet || hit.url }}</div>
        </div>
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
        >{{ part.output }}</pre
      >
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { AgentPart } from '@/types/agent'
import { showToolBody, toolDetail, toolIcon, toolLabel } from './view-logic'

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
/* 思考 / 工具：纯文本行，无卡片描边；动效在文字与图标上 */
.agent-step {
  animation: agent-step-in 0.3s cubic-bezier(0, 0, 0.2, 1) both;
}

.agent-step-label {
  color: var(--color-text-secondary);
}

/* 进行中：标签流光字 + 图标脉动 + 三点 */
.agent-step--active .agent-step-label {
  background: linear-gradient(
    100deg,
    var(--color-text-secondary) 0%,
    var(--color-accent) 40%,
    color-mix(in srgb, var(--color-accent) 50%, white) 50%,
    var(--color-accent) 60%,
    var(--color-text-secondary) 100%
  );
  background-size: 220% 100%;
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  animation: agent-step-text-shimmer 2s ease-in-out infinite;
}

.agent-step--active .agent-step-icon {
  animation: agent-step-icon-pulse 1.4s ease-in-out infinite;
}

/* 失败：red-500（与 toast kind:error 一致）；覆盖 .agent-step-label 默认 secondary */
.agent-step--failed .agent-step-label,
.agent-step--failed .agent-step-icon {
  color: rgb(239 68 68);
}

.agent-step-dots {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: 2px;
  /* 相对中文标签略下移，光学居中 */
  transform: translateY(1px);
}

.agent-step-dots i {
  display: block;
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--color-accent);
  opacity: 0.3;
  animation: agent-step-dot 1.2s ease-in-out infinite;
}

.agent-step-dots i:nth-child(2) {
  animation-delay: 0.15s;
}

.agent-step-dots i:nth-child(3) {
  animation-delay: 0.3s;
}

/* 结果区：gutter 与图标同宽，竖线水平居中（对准图标中线） */
.agent-step-body {
  align-items: stretch;
}

.agent-step-gutter {
  display: flex;
  justify-content: center; /* 横轴居中 2px 竖线 */
  align-self: stretch;
  width: 1em;
}

.agent-step-rail {
  width: 2px;
  border-radius: 1px;
  background: var(--color-divider);
}

.agent-step-out {
  flex: 1 1 0%;
  min-width: 0;
  margin: 0;
  padding: 0;
  animation: agent-step-out-in 0.25s cubic-bezier(0, 0, 0.2, 1) both;
}

@keyframes agent-step-in {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes agent-step-text-shimmer {
  0% {
    background-position: 100% 0;
  }
  100% {
    background-position: -100% 0;
  }
}

@keyframes agent-step-icon-pulse {
  0%,
  100% {
    opacity: 0.7;
    transform: scale(1);
  }
  50% {
    opacity: 1;
    transform: scale(1.1);
  }
}

@keyframes agent-step-dot {
  0%,
  80%,
  100% {
    opacity: 0.25;
    transform: translateY(0);
  }
  40% {
    opacity: 1;
    transform: translateY(-3px);
  }
}

@keyframes agent-step-out-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
</style>
