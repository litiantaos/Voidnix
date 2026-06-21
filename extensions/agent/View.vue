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

            <!-- 工具调用 part：状态图标 + 工具名 + 参数明细 + 状态 -->
            <div
              v-else-if="part.type === 'toolCall'"
              rounded="md"
              bg="black/4"
              p="2.5"
              text="xs"
              flex
              items="center"
              gap="1.5"
            >
              <span shrink="0" :class="toolStateIcon(part.state)" />
              <span shrink="0" text="tx-faint">{{ part.name }}</span>
              <span shrink-1 min-w="0" font="mono" text="tx-secondary" truncate>{{
                toolDetail(part)
              }}</span>
              <span shrink="0" text="tx-faint">·</span>
              <span shrink="0" :class="toolStateTextClass(part.state)">
                {{ toolStateLabel(part.state) }}
              </span>
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
          :placeholder="agent.isGenerating.value ? '执行中，Ctrl+C 中止' : '聊点什么...'"
          class="flex-1 pointer-events-auto !bg-[#f2f2f2]"
          @submit="handleSubmit"
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
        <p text="sm">Agent 想执行工具：</p>
        <p font="bold">{{ agent.pendingApproval.value.toolName }}</p>
        <pre
          p="2"
          bg="black/4"
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
import { computed, ref, nextTick, onMounted, onUnmounted } from 'vue'
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
/* markdown 渲染：紧凑型，针对窄聊天面板调过间距；颜色走主题 token */

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
  line-height: 1.3;
  margin: 1.1em 0 0.45em;
}
.markdown-body :deep(h1) {
  font-size: 1.2em;
}
.markdown-body :deep(h2) {
  font-size: 1.1em;
}
.markdown-body :deep(h3) {
  font-size: 1.05em;
}
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
}

/* 块级统一底间距 + 末元素清零 */
.markdown-body :deep(p),
.markdown-body :deep(ul),
.markdown-body :deep(ol),
.markdown-body :deep(pre),
.markdown-body :deep(blockquote),
.markdown-body :deep(table) {
  margin: 0 0 0.6em;
}
.markdown-body :deep(:last-child) {
  margin-bottom: 0;
}

/* 列表 */
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 1.4em;
}
.markdown-body :deep(li) {
  margin: 0.15em 0;
}
.markdown-body :deep(li > ul),
.markdown-body :deep(li > ol) {
  margin: 0.15em 0;
}

/* 行内代码（不含 pre 内）*/
.markdown-body :deep(:not(pre) > code) {
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  font-size: 0.875em;
  background: rgba(0, 0, 0, 0.05);
  padding: 0.12em 0.38em;
  border-radius: 4px;
  word-break: break-all;
}

/* 代码块 */
.markdown-body :deep(pre) {
  background: rgba(0, 0, 0, 0.05);
  padding: 0.7em 0.85em;
  border-radius: 8px;
  overflow-x: auto;
  font-size: 0.875em;
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

/* 引用块：accent 色左边框 */
.markdown-body :deep(blockquote) {
  border-left: 3px solid var(--color-accent);
  padding-left: 0.85em;
  color: var(--color-tx-secondary);
}

/* 表格（gfm）*/
.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  font-size: 0.95em;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid rgba(0, 0, 0, 0.1);
  padding: 0.35em 0.6em;
  text-align: left;
}
.markdown-body :deep(th) {
  font-weight: 600;
  background: rgba(0, 0, 0, 0.03);
}

/* 水平线 / 图片 / 强调 */
.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.1);
  margin: 1em 0;
}
.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: 6px;
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
