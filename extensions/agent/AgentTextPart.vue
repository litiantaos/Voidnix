<template>
  <div
    class="markdown-body agent-text-in"
    text="sm primary"
    leading="relaxed"
    @click="onMarkdownClick"
  >
    <template v-if="streaming">
      <div v-if="view.solid" class="md-solid" v-html="renderSolidMarkdown(view.solid)" />
      <div class="md-tail" :key="view.lineKey">
        <span class="md-tail-text">{{ view.tail }}</span>
      </div>
    </template>
    <div v-else class="md-full" v-html="renderMarkdown(text)" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { writeText } from '@/utils/clipboard'
import { showToast } from '@/composables/useToast'
import { renderMarkdown } from '@/utils/markdown'
import { renderSolidMarkdown, streamView } from './view-logic'
import { t } from '@/runtime/i18n'

const props = defineProps<{
  text: string
  /** 是否为该消息中正在流式输出的最后一个 text part */
  streaming: boolean
}>()

const view = computed(() => streamView(props.text))

/** 代码块复制：事件委托，读 pre code 文本 */
async function onMarkdownClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  const btn = target?.closest?.('.md-code-copy') as HTMLElement | null
  if (!btn) return
  e.preventDefault()
  e.stopPropagation()

  const root = btn.closest('.md-code')
  const code = root?.querySelector('pre code')?.textContent ?? ''
  if (!code) return

  try {
    await writeText(code)
    const icon = btn.querySelector('i')
    btn.classList.add('is-copied')
    if (icon) icon.className = 'i-ri-check-line'
    showToast(t('common.copied'))
    window.setTimeout(() => {
      btn.classList.remove('is-copied')
      if (icon) icon.className = 'i-ri-file-copy-line'
    }, 1500)
  } catch {
    showToast(t('agent.copyFailed'), { kind: 'error' })
  }
}
</script>

<style scoped>
/* markdown-body 基础样式已下沉为全局（src/styles/markdown.css），此处仅保留 agent 流式专属 */

/* 入场：略上移淡入（仅挂载一次） */
.agent-text-in {
  animation: agent-md-in 0.35s var(--ease-out) both;
}

/*
 * 流式末行拖尾（只在 .md-tail 上）
 * - mask：末端透明渐隐
 * - ::after：仅末尾一截轻模糊
 * - 新行 key 变化：下移淡入（transform 不占布局）
 */
.md-tail {
  position: relative;
  margin: 0;
  min-height: 1.65em;
  line-height: 1.65;
  animation: agent-md-line-in 0.38s var(--ease-out) both;
}

.md-tail-text {
  display: inline;
  white-space: pre-wrap;
  word-break: break-word;
  -webkit-mask-image: linear-gradient(
    90deg,
    #000 0%,
    #000 max(0%, calc(100% - 3.5em)),
    rgba(0, 0, 0, 0.45) calc(100% - 1.4em),
    transparent 100%
  );
  mask-image: linear-gradient(
    90deg,
    #000 0%,
    #000 max(0%, calc(100% - 3.5em)),
    rgba(0, 0, 0, 0.45) calc(100% - 1.4em),
    transparent 100%
  );
}

.md-tail::after {
  content: '';
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(2.8em, 40%);
  pointer-events: none;
  backdrop-filter: blur(1.5px);
  -webkit-backdrop-filter: blur(1.5px);
  -webkit-mask-image: linear-gradient(90deg, transparent 0%, #000 80%);
  mask-image: linear-gradient(90deg, transparent 0%, #000 80%);
}

@keyframes agent-md-in {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* 新行：自上下移就位（不改布局高度） */
@keyframes agent-md-line-in {
  from {
    opacity: 0;
    transform: translateY(-0.45em);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
</style>
