<template>
  <div
    class="markdown-body agent-text-in"
    :class="{ 'markdown-body--streaming': streaming }"
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
import { renderMarkdown, renderSolidMarkdown, streamView } from './view-logic'

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
    showToast('已复制')
    window.setTimeout(() => {
      btn.classList.remove('is-copied')
      if (icon) icon.className = 'i-ri-file-copy-line'
    }, 1500)
  } catch {
    showToast('复制失败', { kind: 'error' })
  }
}
</script>

<style scoped>
/* ── markdown：flex gap 稳间距；流式 = 末行水平拖尾 ── */
.markdown-body {
  display: flex;
  flex-direction: column;
  gap: 0.65em;
  min-width: 0;
  max-width: 100%;
  overflow-wrap: break-word;
  line-height: 1.65;
  color: var(--color-text-primary);
}

/* 入场：略上移淡入（仅挂载一次） */
.agent-text-in {
  animation: agent-md-in 0.35s var(--ease-out) both;
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

/*
 * 标题层级：字号 / 字重 / 色阶区分；h1/h2 左侧竖条 = 字体等高（1em）+ 加粗
 */
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  position: relative;
  margin: 0;
  font-weight: 600;
  line-height: 1.35;
  letter-spacing: -0.01em;
}
.markdown-body :deep(h1) {
  padding-left: 0.7em;
  font-size: 1.28em;
  font-weight: 700;
  color: var(--color-text-primary);
}
.markdown-body :deep(h2) {
  padding-left: 0.65em;
  font-size: 1.14em;
  color: var(--color-text-primary);
}
.markdown-body :deep(h3) {
  font-size: 1.05em;
  font-weight: 600;
  color: var(--color-text-primary);
}
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
  font-weight: 600;
  color: var(--color-text-secondary);
}
/* 竖条：height=1em 与当前标题字号等高；加粗；垂直居中 */
.markdown-body :deep(h1)::before,
.markdown-body :deep(h2)::before {
  content: '';
  position: absolute;
  left: 0;
  top: 50%;
  width: 4px;
  height: 1em;
  border-radius: 2px;
  transform: translateY(-50%);
  background: var(--color-accent);
}
.markdown-body :deep(h2)::before {
  width: 3.5px;
  background: color-mix(in srgb, var(--color-accent) 70%, var(--color-text-muted));
}

/*
 * 列表：renderer 注入固定宽 .md-li-mark + .md-li-body
 * ol/ul 共用同一列宽，正文起点像素级对齐（不靠 ::before/grid）
 */
.markdown-body :deep(ul),
.markdown-body :deep(ol),
.markdown-body :deep(.md-list) {
  list-style: none;
  margin: 0;
  padding: 0;
}
.markdown-body :deep(.md-li) {
  display: flex;
  align-items: baseline;
  /* 有序默认紧凑；无序圆点视觉更轻，间距略加大 */
  gap: 0.35em;
  margin: 0.2em 0;
  padding: 0;
}
.markdown-body :deep(ul > .md-li) {
  gap: 0.55em;
}
.markdown-body :deep(.md-li-mark) {
  box-sizing: border-box;
  flex: 0 0 auto;
  font-family: var(--font-mono);
  font-size: 0.85em;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
  line-height: inherit;
  color: var(--color-text-muted);
  text-align: left;
  white-space: nowrap;
  user-select: none;
}
.markdown-body :deep(.md-li-body) {
  flex: 1 1 0%;
  min-width: 0;
}
/* loose list 的 p：去 UA 边距，避免视觉跳动 */
.markdown-body :deep(.md-li-body > p) {
  margin: 0;
}
.markdown-body :deep(.md-li-body > .md-list) {
  margin-top: 0.15em;
}

/* 行内代码 / 代码块 / 引用：accent 浅染统一走 --accent-wash* / --accent-line* */
.markdown-body :deep(:not(pre) > code) {
  font-family: var(--font-mono);
  font-size: 0.88em;
  background: var(--accent-wash);
  padding: 0.12em 0.4em;
  border-radius: calc(var(--radius-ctrl) / 2);
  word-break: break-all;
}

.markdown-body :deep(.md-code) {
  display: flex;
  flex-direction: column;
  min-width: 0;
  max-width: 100%;
  overflow: hidden;
  border: 1px solid var(--accent-line);
  border-radius: var(--radius-ctrl);
  background: var(--accent-wash-grad);
}
.markdown-body :deep(.md-code-bar) {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 30px;
  padding: 4px 6px 4px var(--space);
  border-bottom: 1px solid var(--accent-line-soft);
  background: transparent;
}
.markdown-body :deep(.md-code-lang) {
  min-width: 0;
  overflow: hidden;
  font-family: var(--font-mono);
  font-size: 11px;
  /* line-height:1 + overflow:hidden 会裁切 mono 字形下沿 */
  line-height: 1.35;
  color: var(--color-text-muted);
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: none;
}
.markdown-body :deep(.md-code-lang--empty) {
  visibility: hidden;
}
.markdown-body :deep(.md-code-copy) {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  margin: 0;
  padding: 0;
  border: none;
  border-radius: calc(var(--radius-ctrl) * 0.85);
  background: transparent;
  color: var(--color-text-muted);
  font-size: 13px;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}
.markdown-body :deep(.md-code-copy:hover) {
  background: var(--color-fill-5);
  color: var(--color-text-secondary);
}
.markdown-body :deep(.md-code-copy:active) {
  background: var(--color-fill-8);
}
.markdown-body :deep(.md-code-copy.is-copied) {
  color: var(--color-accent);
}
.markdown-body :deep(.md-code-copy i) {
  display: block;
  width: 1em;
  height: 1em;
}
.markdown-body :deep(.md-code-pre),
.markdown-body :deep(pre) {
  max-width: 100%;
  max-height: 16rem;
  margin: 0;
  padding: 10px var(--space);
  overflow: auto;
  border: none;
  border-radius: 0;
  background: transparent;
  font-size: 12px;
  line-height: 1.55;
  overscroll-behavior: contain;
}
.markdown-body :deep(pre:not(.md-code-pre)) {
  border: 1px solid var(--accent-line);
  border-radius: var(--radius-ctrl);
  background: var(--accent-wash-grad);
}
.markdown-body :deep(pre code) {
  display: block;
  width: max-content;
  min-width: 100%;
  max-width: none;
  padding: 0;
  border: none;
  border-radius: 0;
  background: none;
  font-family: var(--font-mono);
  font-size: inherit;
  line-height: inherit;
  color: var(--color-text-primary);
  white-space: pre;
  word-break: normal;
  overflow-wrap: normal;
}

/* 引用：与代码块同系 accent 浅染 */
.markdown-body :deep(blockquote) {
  padding: 0.45em 0.75em;
  border: 1px solid var(--accent-line);
  border-radius: var(--radius-ctrl);
  background: var(--accent-wash-grad);
  color: var(--color-text-primary);
  font-size: inherit;
  line-height: 1.65;
}
.markdown-body :deep(blockquote p) {
  margin: 0;
}

/* 表格：撑满容器；单元可换行 */
.markdown-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 0.95em;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border-bottom: 1px solid var(--color-divider);
  padding: 6px 8px;
  text-align: left;
  word-break: break-word;
  overflow-wrap: anywhere;
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
  transition: opacity var(--duration-fast) var(--ease-out);
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
