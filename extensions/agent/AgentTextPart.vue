<template>
  <div
    class="markdown-body agent-text-in"
    :class="{ 'markdown-body--streaming': streaming }"
    text="sm primary"
    leading="relaxed"
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
import { renderMarkdown, renderSolidMarkdown, streamView } from './view-logic'

const props = defineProps<{
  text: string
  /** 是否为该消息中正在流式输出的最后一个 text part */
  streaming: boolean
}>()

const view = computed(() => streamView(props.text))
</script>

<style scoped>
/* ── markdown：flex gap 稳间距；流式 = 末行水平拖尾 ── */
.markdown-body {
  display: flex;
  flex-direction: column;
  gap: 0.65em;
  overflow-wrap: break-word;
  line-height: 1.65;
  color: var(--color-text-primary);
}

/* 入场：略上移淡入（仅挂载一次） */
.agent-text-in {
  animation: agent-md-in 0.35s cubic-bezier(0, 0, 0.2, 1) both;
}

/* solid/full 的块级子节点直接参与 markdown-body 的 gap */
.md-solid,
.md-full {
  display: contents;
}

/* 直子块：margin 清零，间距只走父级 gap */
.markdown-body :deep(> *) {
  margin: 0;
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
  animation: agent-md-line-in 0.38s cubic-bezier(0, 0, 0.2, 1) both;
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

/* 标题：窄面板层级压缩；顶距用 padding 不破坏 gap */
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-weight: 600;
  line-height: 1.35;
  letter-spacing: -0.01em;
  margin: 0;
  padding-top: 0.25em;
  color: var(--color-text-primary);
}
.markdown-body :deep(h1:first-child),
.markdown-body :deep(h2:first-child),
.markdown-body :deep(h3:first-child),
.markdown-body :deep(h4:first-child),
.markdown-body :deep(h5:first-child),
.markdown-body :deep(h6:first-child) {
  padding-top: 0;
}
.markdown-body :deep(h1) {
  font-size: 1.25em;
}
.markdown-body :deep(h2) {
  font-size: 1.12em;
}
.markdown-body :deep(h3) {
  font-size: 1.05em;
}
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 1.25em;
}
.markdown-body :deep(ul) {
  list-style: disc;
}
.markdown-body :deep(ol) {
  list-style: decimal;
}
.markdown-body :deep(li) {
  margin: 0.2em 0;
  padding-left: 0.15em;
}
.markdown-body :deep(li::marker) {
  color: var(--color-text-muted);
}
.markdown-body :deep(li > ul),
.markdown-body :deep(li > ol) {
  margin: 0.15em 0;
}

/* 行内代码：轻描边，忌厚重灰底 */
.markdown-body :deep(:not(pre) > code) {
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  font-size: 0.88em;
  background: var(--color-fill-4);
  border: 1px solid var(--color-divider);
  padding: 0.1em 0.35em;
  border-radius: calc(var(--radius-ctrl) / 2);
  word-break: break-all;
}

/* 代码块：描边 + 极浅底 */
.markdown-body :deep(pre) {
  background: color-mix(in srgb, var(--color-fill-4) 80%, transparent);
  border: 1px solid var(--color-border);
  padding: 10px 12px;
  border-radius: var(--radius-ctrl);
  overflow-x: auto;
  font-size: 12px;
  line-height: 1.55;
}
.markdown-body :deep(pre code) {
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  background: none;
  border: none;
  padding: 0;
  border-radius: 0;
  font-size: inherit;
  word-break: normal;
  color: var(--color-text-secondary);
}

/* 引用：左边线，无灰底（margin 由父 gap 管） */
.markdown-body :deep(blockquote) {
  padding: 0.1em 0 0.1em 12px;
  border-left: 2px solid var(--color-border);
  color: var(--color-text-secondary);
  background: none;
  border-radius: 0;
  font-size: inherit;
  line-height: 1.65;
}
.markdown-body :deep(blockquote p) {
  margin: 0;
}

/* 表格 */
.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  font-size: 0.95em;
  display: block;
  overflow-x: auto;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border-bottom: 1px solid var(--color-divider);
  padding: 6px 8px;
  text-align: left;
}
.markdown-body :deep(th) {
  font-weight: 600;
  color: var(--color-text-secondary);
  background: transparent;
}
.markdown-body :deep(tr:last-child td) {
  border-bottom: none;
}

/* 水平线 / 图片 / 强调 */
.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--color-divider);
  margin: 0;
  width: 100%;
}
.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: var(--radius-ctrl);
}
.markdown-body :deep(strong) {
  font-weight: 600;
}
.markdown-body :deep(em) {
  font-style: italic;
  color: var(--color-text-secondary);
}

/* 链接 */
.markdown-body :deep(a) {
  color: var(--color-accent);
  text-decoration: none;
  transition: opacity 0.15s cubic-bezier(0, 0, 0.2, 1);
}
.markdown-body :deep(a:hover) {
  opacity: 0.8;
  text-decoration: underline;
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
