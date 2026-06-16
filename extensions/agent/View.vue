<template>
  <BaseEmptyState v-if="!isConfigured" icon="i-ri-settings-3-line" title="请先配置 AI Provider" />

  <div v-else p="x-5 t-5" flex="~ col" h="full" min-h="0" @click="onContentClick">
    <div
      v-if="displayMessages.length > 0"
      flex="~ 1 col"
      gap="3"
      min-h="0"
      overflow="y-auto"
      pb="3"
    >
      <template v-for="msg in displayMessages" :key="msg.id">
        <!-- 用户消息 -->
        <div
          v-if="msg.role === 'user'"
          text="sm tx-primary"
          leading="relaxed"
          p="l-3"
          border="l-2 accent"
          whitespace="pre-wrap"
          break="words"
        >
          {{ getText(msg) }}
        </div>

        <!-- assistant 消息（含多 part）-->
        <div v-else flex="~ col" gap="2" p="l-3" border="l-2 tx-faint/20">
          <!-- streaming 占位 -->
          <div
            v-if="msg.streaming && msg.parts.length === 0"
            text="sm tx-subtle"
            flex
            items="center"
            gap="2"
          >
            <span class="i-ri-loader-4-line animate-spin" text="tx-muted" />
            <span>思考中…</span>
          </div>

          <template v-for="(part, i) in msg.parts" :key="i">
            <!-- 文本 part -->
            <div
              v-if="part.type === 'text'"
              class="markdown-body"
              text="sm tx-primary"
              leading="relaxed"
              v-html="renderMarkdown(part.text)"
            />

            <!-- 工具调用 part -->
            <div
              v-else-if="part.type === 'toolCall'"
              rounded="md"
              bg="black/[0.03]"
              p="2.5"
              text="xs"
              flex="~ col"
              gap="1"
            >
              <div flex items="center" gap="1.5">
                <span shrink="0" :class="toolStateIcon(part.state)" />
                <span font="medium" text="tx-secondary">{{ part.name }}</span>
                <span text="tx-faint">·</span>
                <span :class="toolStateTextClass(part.state)">
                  {{ toolStateLabel(part.state) }}
                </span>
              </div>

              <!-- 参数（streaming/running/awaiting 时显示）-->
              <pre
                v-if="
                  part.args !== undefined &&
                  ['streaming', 'running', 'awaiting_approval'].includes(part.state)
                "
                m="0"
                text="tx-subtle"
                font="mono"
                whitespace="pre-wrap"
                break="all"
                >{{ formatArgs(part.args) }}</pre
              >

              <!-- 结果（done/failed 时显示）-->
              <div
                v-if="part.output && ['done', 'failed'].includes(part.state)"
                :class="part.state === 'failed' ? 'text-red-600' : 'text-tx-subtle'"
                max-h="60"
                overflow="auto"
              >
                <!-- web_search 结果用结构化 HTML 渲染（snippet 已转义）-->
                <div
                  v-if="part.name === 'web_search' && part.ok"
                  class="search-results-container"
                  v-html="renderSearchOutput(part.output)"
                />
                <!-- 其他工具：等宽原样显示 -->
                <pre
                  v-else
                  m="0"
                  font="mono"
                  text="xs"
                  whitespace="pre-wrap"
                  break="all"
                >{{ part.output }}</pre>
              </div>
            </div>
          </template>
        </div>
      </template>
    </div>

    <div
      p="y-5"
      pointer-events="none"
      bottom="0"
      sticky
      class="from-transparent to-surface via-surface/60 via-30% bg-linear-to-b"
    >
      <div v-if="displayMessages.length === 0" p="y-6" space="y-1">
        <h1 text="xl" font="bold">来点有意思的吧！</h1>
        <p text="xs tx-muted">日常问题、工作任务、搜索资料、跑命令...</p>
      </div>

      <div flex items="end" gap="2">
        <BaseTextarea
          ref="textareaRef"
          v-model="inputText"
          :placeholder="agent.isGenerating.value ? 'agent 执行中...（可预输入下一条）' : '聊点什么...'"
          class="bg-black/5 pointer-events-auto flex-1"
          @submit="handleSubmit"
        />
        <BaseButton
          v-if="agent.isGenerating.value"
          icon="i-ri-stop-circle-line"
          class="text-red-500 pointer-events-auto"
          @click="agent.abort()"
        />
      </div>
    </div>

    <!-- Approval 弹窗 -->
    <BaseDialog
      v-if="agent.pendingApproval.value"
      title="工具调用审批"
      variant="confirm"
      kind="warning"
      size="md"
      show-footer
      ok-label="执行"
      @confirm="handleApprove(true, false)"
      @cancel="handleApprove(false, false)"
    >
      <div flex="~ col" gap="2">
        <p text="sm">agent 想执行工具：</p>
        <p font="bold">{{ agent.pendingApproval.value.toolName }}</p>
        <pre
          p="2"
          bg="black/[0.03]"
          rounded="md"
          text="xs"
          font="mono"
          whitespace="pre-wrap"
          break="all"
          max-h="60"
          overflow="auto"
          >{{ formatArgs(agent.pendingApproval.value.args) }}</pre
        >
      </div>
      <template #footer-start>
        <BaseButton class="text-green-600" @click="handleApprove(true, true)">
          执行并信任
        </BaseButton>
      </template>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, onMounted } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
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

const settings = useSettingsStore()
const agent = useAgentChat()
const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const inputText = ref('')

const displayMessages = computed(() => agent.messages.value)

const isConfigured = computed(
  () => settings.activeProviderConfig.endpoint && settings.activeProviderConfig.apiKey,
)

function getText(msg: AgentMessage): string {
  return msg.parts
    .filter((p): p is Extract<AgentPart, { type: 'text' }> => p.type === 'text')
    .map((p) => p.text)
    .join('')
}

function formatArgs(args: unknown): string {
  if (args === undefined || args === null) return ''
  if (typeof args === 'string') return args
  try {
    return JSON.stringify(args, null, 2)
  } catch {
    return String(args)
  }
}

/// 把 web_search 的 JSON 输出转成 HTML（snippet 转义，避免误渲染 markdown）
/// 链接加 target + 拦截 click 由系统浏览器打开（不在 webview 内导航）
function renderSearchOutput(raw: string): string {
  interface SearchHit {
    title: string
    url: string
    snippet: string
  }
  interface SearchOutcome {
    answer?: string | null
    hits?: SearchHit[]
  }
  let data: SearchOutcome
  try {
    data = JSON.parse(raw)
  } catch {
    return `<pre>${escapeHtml(raw)}</pre>`
  }

  const parts: string[] = []
  if (data.answer) {
    parts.push(`<blockquote>${escapeHtml(data.answer.trim())}</blockquote>`)
  }
  if (data.hits?.length) {
    parts.push('<ol class="search-results">')
    for (const h of data.hits) {
      const title = escapeHtml(h.title || '(无标题)')
      const url = escapeHtml(h.url || '')
      const snippet = h.snippet?.trim()
      const snippetHtml = snippet ? `<small class="search-snippet">${escapeHtml(snippet)}</small>` : ''
      parts.push(
        `<li><a href="${url}" target="_blank" rel="noopener noreferrer" class="search-link">${title}</a>${snippetHtml}</li>`,
      )
    }
    parts.push('</ol>')
  } else if (!data.answer) {
    parts.push('<p><em>无搜索结果</em></p>')
  }
  return DOMPurify.sanitize(parts.join(''), {
    ADD_ATTR: ['target', 'rel'],
  })
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function toolStateIcon(state: string): string {
  switch (state) {
    case 'streaming':
    case 'running':
      return 'i-ri-loader-4-line animate-spin text-accent'
    case 'done':
      return 'i-ri-checkbox-circle-line text-green-600'
    case 'failed':
      return 'i-ri-close-circle-line text-red-500'
    case 'awaiting_approval':
      return 'i-ri-question-line text-amber-600'
    default:
      return 'i-ri-tools-line text-tx-muted'
  }
}

function toolStateTextClass(state: string): string {
  switch (state) {
    case 'done':
      return 'text-green-600'
    case 'failed':
      return 'text-red-500'
    case 'awaiting_approval':
      return 'text-amber-600'
    default:
      return 'text-tx-faint'
  }
}

function toolStateLabel(state: string): string {
  switch (state) {
    case 'streaming':
      return '解析参数'
    case 'running':
      return '执行中'
    case 'awaiting_approval':
      return '等待审批'
    case 'done':
      return '完成'
    case 'failed':
      return '失败'
    default:
      return state
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

async function handleApprove(approved: boolean, alwaysApprove: boolean) {
  await agent.approve(approved, alwaysApprove)
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
  try {
    const { open } = await import('@tauri-apps/plugin-shell')
    await open(href)
  } catch (err) {
    console.warn('Failed to open in browser:', err)
  }
}

onMounted(() => nextTick(() => textareaRef.value?.focus()))
</script>

<style scoped>
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-weight: 600;
  margin-top: 1em;
  margin-bottom: 0.5em;
}

.markdown-body :deep(h1) {
  font-size: 1.5em;
}
.markdown-body :deep(h2) {
  font-size: 1.3em;
}
.markdown-body :deep(h3) {
  font-size: 1.1em;
}
.markdown-body :deep(p) {
  margin-bottom: 0.75em;
}
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 1.5em;
  margin-bottom: 0.75em;
}
.markdown-body :deep(li) {
  margin-bottom: 0.25em;
}
.markdown-body :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  padding: 0.15em 0.4em;
  border-radius: 4px;
  font-size: 0.9em;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
}
.markdown-body :deep(pre) {
  background: rgba(0, 0, 0, 0.06);
  padding: 1em;
  border-radius: 6px;
  overflow-x: auto;
  margin-bottom: 0.75em;
}
.markdown-body :deep(pre code) {
  background: none;
  padding: 0;
  font-size: 0.85em;
}
.markdown-body :deep(blockquote) {
  border-left: 3px solid rgba(0, 0, 0, 0.1);
  padding-left: 1em;
  color: rgba(0, 0, 0, 0.6);
  margin-bottom: 0.75em;
}
.markdown-body :deep(a) {
  color: var(--color-accent);
  text-decoration: none;
}
.markdown-body :deep(a:hover) {
  text-decoration: underline;
}
.markdown-body :deep(*:last-child) {
  margin-bottom: 0;
}

/* 搜索结果容器 */
.search-results-container :deep(ol) {
  margin: 0;
  padding-left: 1.5em;
}
.search-results-container :deep(li) {
  margin-bottom: 0.4em;
  line-height: 1.4;
}
.search-results-container :deep(.search-link) {
  color: var(--color-accent);
  text-decoration: none;
  font-weight: 500;
}
.search-results-container :deep(.search-link:hover) {
  text-decoration: underline;
}
.search-results-container :deep(.search-snippet) {
  display: block;
  color: rgba(0, 0, 0, 0.5);
  font-size: 0.95em;
  margin-top: 0.15em;
}
.search-results-container :deep(blockquote) {
  border-left: 3px solid var(--color-accent);
  padding-left: 0.75em;
  margin: 0 0 0.5em;
  color: rgba(0, 0, 0, 0.7);
}
</style>
