<template>
  <BaseEmptyState
    v-if="!isConfigured"
    icon="i-ri-settings-3-line"
    title="请先配置 AI Provider 与模型"
  />

  <div v-else class="agent-layout" @click="onContentClick">
    <!-- 空态：居中（顶部留白补偿搜索栏）-->
    <div v-if="displayMessages.length === 0" class="agent-empty">
      <h1 text="xl" font="bold">来点有意思的吧！</h1>
      <p text="xs muted">日常问题、工作任务、搜索资料、跑命令...</p>
    </div>

    <!-- 消息滚动区：填满可视区，向上滚动消息进入搜索栏下层 -->
    <div v-else ref="scrollRef" class="agent-scroll hide-scrollbar" @scroll.passive="onScroll">
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
          {{ getMessageText(msg) }}
        </div>

        <!-- assistant 消息（含多 part）-->
        <div v-else flex="~ col" gap="2" p="l-3" border="l-2 muted/20">
          <!-- streaming 占位：纯文本行 + 动效（无卡片） -->
          <div
            v-if="msg.streaming && msg.parts.length === 0"
            class="agent-step agent-step--active"
            text="xs"
            flex
            items="center"
            gap="1.5"
            min-w="0"
          >
            <span shrink="0" class="agent-step-icon i-ri-sparkling-2-line" text="accent" />
            <span class="agent-step-label" shrink="0" font="medium">思考</span>
            <span class="agent-step-dots" aria-hidden="true"> <i /><i /><i /> </span>
          </div>

          <template v-for="(part, i) in msg.parts" :key="i">
            <AgentTextPart
              v-if="part.type === 'text'"
              :text="part.text"
              :streaming="isStreamingText(msg, i)"
            />
            <AgentToolStep
              v-else-if="part.type === 'toolCall'"
              :part="part"
              :index="i"
              @open-url="openUrl"
            />
          </template>
        </div>
      </template>
    </div>

    <div class="agent-footer">
      <!-- 输出中：滚底圆 / 中止胶囊（内光 Siri 感），单层 BaseButton + 进出场 -->
      <Transition
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0 translate-y-5 scale-90"
        enter-to-class="opacity-100 translate-y-0 scale-100"
        leave-active-class="transition duration-150 ease-in"
        leave-from-class="opacity-100 translate-y-0 scale-100"
        leave-to-class="opacity-0 translate-y-4 scale-90"
      >
        <div v-if="agent.isGenerating.value" class="agent-float-actions">
          <BaseButton
            class="agent-float-scroll"
            icon="i-ri-arrow-down-s-line"
            @click="scrollToBottom"
          />
          <BaseButton class="agent-float-stop" icon="i-ri-stop-mini-fill" @click="agent.abort()" />
        </div>
      </Transition>

      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        class="agent-input"
        rounded="panel"
        :placeholder="agent.isGenerating.value ? '执行中，Ctrl+C 中止' : '聊点什么...'"
        @submit="handleSubmit"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, onMounted, onUnmounted, watch } from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { isProviderReady } from './config'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import AgentTextPart from './AgentTextPart.vue'
import AgentToolStep from './AgentToolStep.vue'
import { useAgentChat } from './agent'
import { getMessageText, isStreamingText, streamLayoutKey } from './view-logic'

/** 距底部多少 px 内视为贴底（自动滚底生效） */
const NEAR_BOTTOM_PX = 24
const MAX_INPUT_LENGTH = 8192

const agent = useAgentChat()
const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const scrollRef = ref<HTMLElement>()
const inputText = ref('')
/** 用户是否贴底：仅贴底时 streaming 增量才自动滚底，上翻阅读时不打断 */
const stickToBottom = ref(true)

const displayMessages = computed(() => agent.messages.value)
const isConfigured = isProviderReady

function isNearBottom(el: HTMLElement): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX
}

function onScroll() {
  const el = scrollRef.value
  if (!el) return
  stickToBottom.value = isNearBottom(el)
}

function scrollToBottom() {
  const el = scrollRef.value
  if (!el) return
  el.scrollTop = el.scrollHeight
  stickToBottom.value = true
}

let scrollRaf = 0
function scheduleScrollToBottom() {
  if (scrollRaf) return
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    if (stickToBottom.value) scrollToBottom()
  })
}

/// 新消息 / streaming 增量：rAF 合并贴底滚底（流式阶段 height:auto，无逐帧高度插值）
watch(
  () => streamLayoutKey(agent.messages.value, agent.isGenerating.value),
  async () => {
    await nextTick()
    scheduleScrollToBottom()
  },
)

async function handleSubmit() {
  const text = inputText.value.trim()
  if (!text || agent.isGenerating.value) return
  const clipped = text.length > MAX_INPUT_LENGTH ? text.slice(0, MAX_INPUT_LENGTH) : text
  inputText.value = ''
  // 发送后跟读最新输出
  stickToBottom.value = true
  await agent.sendMessage(clipped)
  await nextTick()
  scrollToBottom()
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
onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
})
</script>

<style scoped>
/* 绝对定位填充 scrollContainer（突破 paddingTop），
   让消息滚至搜索栏和输入框下层 */
.agent-layout {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 空态：在搜索栏和输入框之间居中 */
.agent-empty {
  flex: 1 1 0%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  text-align: center;
  padding-top: var(--chrome-fade-height);
}

/* 消息滚动区：填满可视区，向上滚动消息进入搜索栏（z-10）下层 */
.agent-scroll {
  flex: 1 1 0%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
  overflow-y: auto;
  padding: var(--chrome-fade-height) 12px 12px;
}

/* 底部：输入框 + 输出中悬浮操作；外边距由 footer 承担 */
.agent-footer {
  position: relative;
  flex: none;
  margin: 0 12px 12px;
}

/* ui-ctrl 默认 border-none + focus-within ring；有默认描边时关 ring，聚焦只改边框色，避免双线 */
.agent-footer :deep(.agent-input) {
  border: 1px solid var(--color-border);
  transition: border-color 150ms cubic-bezier(0, 0, 0.2, 1);
}

.agent-footer :deep(.agent-input:focus-within) {
  border-color: color-mix(in srgb, var(--color-accent) 50%, transparent);
  /* 清掉 wind4 ring 相关 shadow（ui-ctrl: focus-within:ring-1 ring-inset） */
  --un-ring-shadow: 0 0 #0000;
  --un-inset-ring-shadow: 0 0 #0000;
  --un-ring-offset-shadow: 0 0 #0000;
  box-shadow: none !important;
}

/* 输入框上方居中；translate 与进出场 transform 解耦 */
.agent-float-actions {
  position: absolute;
  bottom: 100%;
  left: 50%;
  translate: -50% 0;
  margin-bottom: 8px;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 8px;
}

/*
 * 中止按钮 aurora 依赖 @property 插值自定义属性。
 * 现代 WKWebView（较新 macOS）可用；不支持时退化为静态多层 background，功能不受影响。
 */
@property --agent-ai-angle {
  syntax: '<angle>';
  initial-value: 0deg;
  inherits: false;
}
@property --ax {
  syntax: '<percentage>';
  initial-value: 28%;
  inherits: false;
}
@property --ay {
  syntax: '<percentage>';
  initial-value: 36%;
  inherits: false;
}
@property --bx {
  syntax: '<percentage>';
  initial-value: 74%;
  inherits: false;
}
@property --by {
  syntax: '<percentage>';
  initial-value: 64%;
  inherits: false;
}
@property --cx {
  syntax: '<percentage>';
  initial-value: 50%;
  inherits: false;
}
@property --cy {
  syntax: '<percentage>';
  initial-value: 70%;
  inherits: false;
}

/*
 * 两钮同高、同 1px 边、同静态外阴影 → 外轮廓对齐。
 * 中止 loading 用多层 background（非 ::before 负 inset），避免 WK button overflow 裁切失败。
 */
.agent-float-actions :deep(.agent-float-scroll),
.agent-float-actions :deep(.agent-float-stop) {
  position: relative;
  box-sizing: border-box;
  height: 30px;
  border: 1px solid color-mix(in srgb, black 8%, transparent);
  border-radius: 999px;
  background: color-mix(in srgb, white 95%, transparent);
  backdrop-filter: blur(8px);
  box-shadow: 0 2px 8px color-mix(in srgb, black 6%, transparent);
}

.agent-float-actions :deep(.agent-float-scroll:hover) {
  background: color-mix(in srgb, white 90%, transparent);
}

/* 滚底：圆 */
.agent-float-actions :deep(.agent-float-scroll) {
  width: 30px;
  min-width: 30px;
  padding: 0;
}

/* 中止：同高胶囊；Siri 内光 = 浅蓝 + 暖杏双斑 + 慢旋 conic（整体偏浅）
 * background-clip: padding-box → 光只画在边框内侧，不盖描边 */
.agent-float-actions :deep(.agent-float-stop) {
  width: auto;
  min-width: 48px;
  padding: 0 14px;
  background:
    radial-gradient(
      circle at var(--ax) var(--ay),
      color-mix(in srgb, var(--color-accent) 28%, transparent) 0%,
      transparent 55%
    ),
    radial-gradient(
      circle at var(--bx) var(--by),
      color-mix(in srgb, var(--color-accent) 16%, white) 0%,
      transparent 50%
    ),
    /* 暖杏斑：装饰渐变，非语义色，不入 theme */
    radial-gradient(
        circle at var(--cx) var(--cy),
        color-mix(in srgb, #ffb089 32%, transparent) 0%,
        transparent 52%
      ),
    conic-gradient(
      from var(--agent-ai-angle),
      transparent 0deg 170deg,
      color-mix(in srgb, var(--color-accent) 22%, white) 210deg,
      color-mix(in srgb, #ffc4a0 26%, white) 255deg,
      color-mix(in srgb, var(--color-accent) 14%, transparent) 300deg,
      transparent 340deg 360deg
    ),
    color-mix(in srgb, white 96%, transparent);
  background-origin: padding-box;
  background-clip: padding-box;
  animation:
    agent-ai-rotate 5.5s linear infinite,
    agent-ai-aurora 3.8s ease-in-out infinite;
}

/* hover 保留多层光，仅略压暗底 */
.agent-float-actions :deep(.agent-float-stop:hover) {
  background:
    radial-gradient(
      circle at var(--ax) var(--ay),
      color-mix(in srgb, var(--color-accent) 28%, transparent) 0%,
      transparent 55%
    ),
    radial-gradient(
      circle at var(--bx) var(--by),
      color-mix(in srgb, var(--color-accent) 16%, white) 0%,
      transparent 50%
    ),
    radial-gradient(
      circle at var(--cx) var(--cy),
      color-mix(in srgb, #ffb089 32%, transparent) 0%,
      transparent 52%
    ),
    conic-gradient(
      from var(--agent-ai-angle),
      transparent 0deg 170deg,
      color-mix(in srgb, var(--color-accent) 22%, white) 210deg,
      color-mix(in srgb, #ffc4a0 26%, white) 255deg,
      color-mix(in srgb, var(--color-accent) 14%, transparent) 300deg,
      transparent 340deg 360deg
    ),
    color-mix(in srgb, white 92%, transparent);
  background-origin: padding-box;
  background-clip: padding-box;
}

.agent-float-actions :deep(.agent-float-scroll i),
.agent-float-actions :deep(.agent-float-stop i) {
  position: relative;
  font-size: 14px;
}

/* 中止图标：实心黑（非 danger 红） */
.agent-float-actions :deep(.agent-float-stop i) {
  color: var(--color-text-primary);
}

@keyframes agent-ai-rotate {
  to {
    --agent-ai-angle: 360deg;
  }
}

@keyframes agent-ai-aurora {
  0%,
  100% {
    --ax: 28%;
    --ay: 36%;
    --bx: 74%;
    --by: 64%;
    --cx: 48%;
    --cy: 72%;
  }
  33% {
    --ax: 52%;
    --ay: 58%;
    --bx: 42%;
    --by: 32%;
    --cx: 68%;
    --cy: 42%;
  }
  66% {
    --ax: 68%;
    --ay: 28%;
    --bx: 30%;
    --by: 72%;
    --cx: 32%;
    --cy: 55%;
  }
}

/* 思考占位（工具步骤样式在 AgentToolStep） */
.agent-step-label {
  color: var(--color-text-secondary);
}

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

.agent-step-dots {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: 2px;
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
</style>
