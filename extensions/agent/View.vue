<template>
  <BaseEmptyState v-if="!isConfigured" icon="i-ri-settings-3-line" title="请先配置 AI Provider" />

  <div v-else flex="~ col" overflow="hidden" @click="onContentClick">
    <!-- 空态：提为顶层 flex-1 兄弟并居中（原塞在 sticky 输入容器内不居中）-->
    <div
      v-if="displayMessages.length === 0"
      flex="~ 1 col"
      items="center"
      justify="center"
      text="center"
      space="y-1"
    >
      <h1 text="xl" font="bold">来点有意思的吧！</h1>
      <p text="xs muted">日常问题、工作任务、搜索资料、跑命令...</p>
    </div>

    <!-- 顶距交给 CHROME_HEIGHT，与列表 px-3 pb-3 同构，勿 p-t 叠双层 -->
    <div
      v-else
      flex="~ 1 col"
      gap="3"
      min-h="0"
      overflow="y-auto"
      p="x-3 b-3"
      class="hide-scrollbar"
    >
      <template v-for="msg in displayMessages" :key="msg.id">
        <!-- 用户消息 -->
        <div
          v-if="msg.role === 'user'"
          text="sm primary"
          leading="relaxed"
          p="l-3"
          border="l-2 accent"
          whitespace="pre-wrap"
          break="words"
        >
          {{ getText(msg) }}
        </div>

        <!-- assistant 消息（含多 part）-->
        <div v-else flex="~ col" gap="2" p="l-3" border="l-2 muted/20">
          <!-- streaming 占位 -->
          <div
            v-if="msg.streaming && msg.parts.length === 0"
            text="sm secondary"
            flex
            items="center"
            gap="2"
          >
            <span class="i-ri-loader-4-line animate-spin" text="muted" />
            <span>思考中…</span>
          </div>

          <template v-for="(part, i) in msg.parts" :key="i">
            <!-- 文本 part -->
            <div
              v-if="part.type === 'text'"
              class="markdown-body"
              text="sm primary"
              leading="relaxed"
              v-html="renderMarkdown(part.text)"
            />

            <!-- 工具调用 part：状态图标 + 工具名 + 参数明细 + 结果 -->
            <div
              v-else-if="part.type === 'toolCall'"
              class="radius-ctrl"
              bg="black/4"
              text="xs"
              flex="~ col"
              overflow="hidden"
            >
              <div p="3" flex items="center" gap="1.5">
                <span shrink="0" font="mono" text="secondary" bg="black/8" rounded px="1">{{
                  part.name
                }}</span>
                <span
                  v-if="toolDetail(part)"
                  shrink-1
                  min-w="0"
                  font="mono"
                  text="secondary"
                  truncate
                  >{{ toolDetail(part) }}</span
                >
                <span shrink="0" ml="auto" :class="toolStateIcon(part.state)" />
              </div>
              <!-- web_search 结果：可点击列表（标题 + 详情）-->
              <div
                v-if="part.parsed && (part.state === 'done' || part.state === 'failed')"
                border="t black/5"
                p="3"
                flex="~ col"
                gap="1.5"
              >
                <div v-for="(hit, i) in part.parsed.hits" :key="i" py="1" @click="openUrl(hit.url)">
                  <div text="primary" truncate class="hover:underline">{{ hit.title }}</div>
                  <div text="secondary" truncate>{{ hit.snippet || hit.url }}</div>
                </div>
              </div>
              <!-- 其他工具结果：pre -->
              <pre
                v-else-if="part.output && (part.state === 'done' || part.state === 'failed')"
                border="t black/5"
                p="3"
                font="mono"
                text="secondary"
                whitespace="pre-wrap"
                break="words"
                max-h="40"
                overflow="auto"
                m="0"
                >{{ part.output }}</pre
              >
            </div>
          </template>
        </div>
      </template>
    </div>

    <div p="x-3 b-3" flex="~ col" shrink="0">
      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        rounded="panel"
        :placeholder="agent.isGenerating.value ? '执行中，Ctrl+C 中止' : '聊点什么...'"
        @submit="handleSubmit"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, onMounted, onUnmounted } from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { activeProviderConfig } from './config'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import { useAgentChat } from './agent'
import type { AgentMessage, AgentPart } from '@/types/agent'

marked.setOptions({ gfm: true, breaks: true })

const renderMarkdown = (content: unknown) => {
  if (typeof content !== 'string' || !content) return ''
  const result = marked.parse(content)
  if (typeof result !== 'string') return ''
  const sanitized = DOMPurify.sanitize(result, { ADD_ATTR: ['target', 'rel'] })
  // 所有 a 标签加 target + rel，避免在 webview 内导航
  return sanitized.replace(/<a\s+href/gi, '<a target="_blank" rel="noopener noreferrer" href')
}

const MAX_INPUT_LENGTH = 8192

const agent = useAgentChat()
const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const inputText = ref('')

const displayMessages = computed(() => agent.messages.value)

const isConfigured = computed(
  () => activeProviderConfig.value.endpoint && activeProviderConfig.value.apiKey,
)

function getText(msg: AgentMessage): string {
  return msg.parts
    .filter((p): p is Extract<AgentPart, { type: 'text' }> => p.type === 'text')
    .map((p) => p.text)
    .join('')
}

/// 工具参数明细：run_command → cmd args…，web_search → query，其余空串。
function toolDetail(part: Extract<AgentPart, { type: 'toolCall' }>): string {
  if (!part.args || typeof part.args !== 'object') return ''
  const obj = part.args as Record<string, unknown>
  if (part.name === 'run_command') {
    const cmd = typeof obj.cmd === 'string' ? obj.cmd : ''
    const argsArr = Array.isArray(obj.args)
      ? obj.args.filter((a): a is string => typeof a === 'string')
      : []
    return [cmd, ...argsArr].filter(Boolean).join(' ')
  }
  if (part.name === 'web_search') {
    return typeof obj.query === 'string' ? obj.query.trim() : ''
  }
  return ''
}

function toolStateIcon(state: string): string {
  switch (state) {
    case 'streaming':
      return 'i-ri-loader-4-line animate-spin text-accent'
    case 'done':
      return 'i-ri-checkbox-circle-line text-green-600'
    case 'failed':
      return 'i-ri-close-circle-line text-red-500'
    case 'running':
      return 'i-ri-loader-4-line animate-spin text-accent'
    default:
      return 'i-ri-tools-line text-muted'
  }
}

async function handleSubmit() {
  const text = inputText.value.trim()
  if (!text || agent.isGenerating.value) return
  if (text.length > MAX_INPUT_LENGTH) inputText.value = text.slice(0, MAX_INPUT_LENGTH)
  inputText.value = ''
  await agent.sendMessage(text)
  nextTick(() => textareaRef.value?.focus())
}

/// 用系统浏览器打开 URL（不在 webview 内导航）
async function openUrl(url: string) {
  if (!url) return
  try {
    await open(url)
  } catch (err) {
    console.warn('Failed to open URL:', err)
  }
}

/// 拦截 markdown-body 内的链接点击：用系统浏览器打开（不在 webview 内导航）
async function onContentClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (target.tagName !== 'A') return
  const anchor = target as HTMLAnchorElement
  const href = anchor.getAttribute('href') || ''
  // 仅拦截 http/https（mailto/tel 等让浏览器默认处理）
  if (!/^https?:\/\//i.test(href)) return
  e.preventDefault()
  e.stopPropagation()
  await openUrl(href)
}

onMounted(() => nextTick(() => textareaRef.value?.focus()))

/// Ctrl+C 中止当前 agent run（macOS 复制是 Cmd+C，Ctrl+C 不冲突）
function onKeydown(e: KeyboardEvent) {
  if (e.ctrlKey && (e.key === 'c' || e.key === 'C')) {
    if (agent.isGenerating.value) {
      e.preventDefault()
      agent.abort()
    }
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<style scoped>
/* markdown 渲染 */

.markdown-body {
  overflow-wrap: break-word;
}

/* 标题：层级压缩，适合窄面板 */
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-weight: 600;
  line-height: 1.6;
  margin: 16px 0 6px;
}
.markdown-body :deep(h1) {
  font-size: 20px;
}
.markdown-body :deep(h2) {
  font-size: 18px;
}
.markdown-body :deep(h3) {
  font-size: 16px;
}
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 14px;
}

/* 块级统一底间距 + 末元素清零 */
.markdown-body :deep(p),
.markdown-body :deep(ul),
.markdown-body :deep(ol),
.markdown-body :deep(pre),
.markdown-body :deep(blockquote),
.markdown-body :deep(table) {
  margin: 0 0 8px;
}
.markdown-body :deep(:last-child) {
  margin-bottom: 0;
}

/* 列表 */
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 20px;
}
.markdown-body :deep(ul) {
  list-style: disc;
}
.markdown-body :deep(ol) {
  list-style: decimal;
}
.markdown-body :deep(li) {
  margin: 2px 0;
}
.markdown-body :deep(li > ul),
.markdown-body :deep(li > ol) {
  margin: 2px 0;
}

/* 行内代码（不含 pre 内）— 填充/圆角走 theme token */
.markdown-body :deep(:not(pre) > code) {
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  font-size: 12px;
  background: var(--color-fill-5);
  padding: 2px 4px;
  border-radius: calc(var(--radius-ctrl) / 2);
  word-break: break-all;
}

/* 代码块 */
.markdown-body :deep(pre) {
  background: var(--color-fill-5);
  padding: 10px 12px;
  border-radius: var(--radius-ctrl);
  overflow-x: auto;
  font-size: 12px;
  line-height: 1.6;
}
.markdown-body :deep(pre code) {
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  background: none;
  padding: 0;
  border-radius: 0;
  font-size: inherit;
  word-break: normal;
}

/* 引用块 */
.markdown-body :deep(blockquote) {
  background: var(--color-fill-5);
  padding: 10px 12px;
  border-radius: var(--radius-ctrl);
  overflow-x: auto;
  font-size: 12px;
  line-height: 1.6;
}

/* 表格（gfm） */
.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  font-size: 14px;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border-bottom: 1px solid var(--color-divider);
  padding: 4px 8px;
  text-align: left;
}
.markdown-body :deep(th) {
  font-weight: 600;
  background: var(--color-fill-4);
}

/* 水平线 / 图片 / 强调 */
.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--color-border);
  margin: 14px 0;
}
.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: calc(var(--radius-ctrl) * 2 / 3);
}
.markdown-body :deep(strong) {
  font-weight: 600;
}

/* 链接 */
.markdown-body :deep(a) {
  color: var(--color-accent);
  text-decoration: none;
}
.markdown-body :deep(a:hover) {
  text-decoration: underline;
}
</style>
