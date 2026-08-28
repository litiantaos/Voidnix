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
      <span class="agent-step-label" shrink="0" font="medium">{{ t('agent.thinking') }}</span>
      <span v-if="streaming" class="agent-step-dots" aria-hidden="true"> <i /><i /><i /> </span>
    </div>
    <div v-if="text" class="agent-step-body" flex gap="1.5" min-w="0">
      <div class="agent-step-gutter" shrink="0" aria-hidden="true">
        <span class="agent-step-rail" />
      </div>
      <!-- 流式：已定行（纯文本）+ 末行拖尾（复用回复正文 .md-tail：渐隐 + 轻模糊 + 新行下移淡入）；
           完成后整体回落为单一纯文本。clamp 贴底对齐使末行拖尾恒在可视区底部 -->
      <p class="agent-step-out agent-step-clamp" text="secondary" leading="snug">
        <template v-if="streaming">
          <span v-if="solid">{{ solid }}</span>
          <span :key="view.lineKey" class="md-tail">
            <span class="md-tail-text">{{ view.tail }}</span>
          </span>
        </template>
        <span v-else>{{ text }}</span>
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import './agent-step.css'
import { computed } from 'vue'
import { t } from '@/runtime/i18n'
import { streamView } from './view-logic'

const props = defineProps<{
  text: string
  /** 是否正在流式输出（最后一个 part 且消息 streaming） */
  streaming: boolean
}>()

const view = computed(() => streamView(props.text))
/** 已定行 = 全文去掉末行拖尾与分隔换行（换行由块级拖尾的起始自然承担，
    保留会在 pre-wrap 下产生空行框把拖尾顶离贴底） */
const solid = computed(() =>
  view.value.tail === props.text
    ? ''
    : props.text.slice(0, props.text.length - view.value.tail.length - 1),
)
</script>
