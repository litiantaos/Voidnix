<template>
  <div
    class="markdown-body agent-text-in"
    text="sm primary"
    leading="relaxed"
    @click="onMarkdownClick"
  >
    <!-- 统一 v-for：流式/收尾共用同一节点序列（仅 class 与末块 HTML 变化），
         收尾时前缀块零 DOM 写；容器 display:contents，子块直接参与 markdown-body 的 gap 布局 -->
    <div
      v-for="(b, i) in blocks"
      :key="i"
      :class="streaming ? 'md-solid' : 'md-full'"
      v-html="renderBlock(b)"
    />
    <div v-if="streaming" class="md-tail" :key="view.lineKey">
      <span class="md-tail-text">{{ view.tail }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { writeText } from '@/utils/clipboard'
import { showToast } from '@/composables/useToast'
import { renderMarkdown } from '@/utils/markdown'
import { streamView, splitStreamBlocks } from './view-logic'
import { t } from '@/runtime/i18n'
// 拖尾样式本体在共享 agent-step.css（与思考正文共用），自持导入不依赖父级
import './agent-step.css'

const props = defineProps<{
  text: string
  /** 是否为该消息中正在流式输出的最后一个 text part */
  streaming: boolean
}>()

const view = computed(() => streamView(props.text))
/** 统一分块：流式取 solid 块（末块活跃），收尾取全量块——前缀块文本一致，
    收尾仅末块 v-html 变化 + tail 卸载，已渲染 DOM 全保留（无整树重建尖峰） */
const blocks = computed(() => (props.streaming ? view.value.blocks : splitStreamBlocks(props.text)))

/// 块级 markdown 缓存：块文本恒定即命中；流式增量与收尾共用（旧实现每行全量重 parse）
const blockCache = new Map<string, string>()
function renderBlock(b: string): string {
  let html = blockCache.get(b)
  if (html === undefined) {
    html = renderMarkdown(b)
    blockCache.set(b, html)
  }
  return html
}

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

/* 入场：略上移淡入（仅挂载一次）。
   backwards 而非 both：both 会把结束帧 transform 永久驻留（有 transform 即合成层），
   每个文本块常驻一个 IOSurface——轻量会话也不经济 */
.agent-text-in {
  animation: agent-md-in 0.35s var(--ease-out) backwards;
}

/* 拖尾本体（mask 渐隐 / 末端轻模糊 / 新行下移淡入）在共享 agent-step.css，
   与思考正文（AgentReasoningPart）共用；此处仅覆盖回复正文的行距微调 */
.md-tail {
  line-height: 1.65;
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
</style>
