<template>
  <div v-if="!isConfigured" class="agent-setup">
    <BaseEmptyState icon="i-ri-key-2-line" :title="t('agent.setupTitle')" />
  </div>

  <div
    v-else
    class="agent-layout"
    :style="{ '--agent-footer-reserve': footerReserve + 'px' }"
    @click="onContentClick"
  >
    <!-- 空态：居中（顶部留白补偿搜索栏）-->
    <div v-if="displayMessages.length === 0" class="agent-empty">
      <h1 text="xl primary" font="bold">{{ t('agent.emptyTitle') }}</h1>
      <p text="xs muted">{{ t('agent.emptyHint') }}</p>
    </div>

    <!-- 消息滚动区：KeepAlive 激活时渲染 DOM，deactivate 时卸载释放 WebKit 内存。
         数据常驻 messages 单例，重新进入时从数据重建。 -->
    <div
      v-else-if="shouldRenderMessages"
      ref="scrollRef"
      class="agent-scroll hide-scrollbar"
      @scroll.passive="onScroll"
    >
      <template v-for="msg in displayMessages" :key="msg.id">
        <!-- 用户消息：右对齐 bubble -->
        <div v-if="msg.role === 'user'" class="agent-row agent-row--user" :data-msg-id="msg.id">
          <div class="agent-bubble agent-bubble--user">
            {{ getMessageText(msg) }}
          </div>
        </div>

        <!-- assistant：左对齐软白卡（soft-surface 与选中/搜索栏同质） -->
        <div v-else class="agent-row agent-row--assistant">
          <div class="agent-card soft-card">
            <!-- streaming 占位 -->
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
              <span class="agent-step-label" shrink="0" font="medium">{{
                t('agent.thinking')
              }}</span>
              <span class="agent-step-dots" aria-hidden="true"> <i /><i /><i /> </span>
            </div>

            <template v-for="(part, i) in msg.parts" :key="partKey(part, i)">
              <AgentReasoningPart
                v-if="part.type === 'reasoning'"
                :text="part.text"
                :streaming="!!msg.streaming && i === msg.parts.length - 1"
              />
              <AgentTextPart
                v-else-if="part.type === 'text'"
                :text="part.text"
                :streaming="isStreamingText(msg, i)"
              />
              <AgentToolStep v-else-if="part.type === 'toolCall'" :part="part" :index="i" />
              <div
                v-else-if="part.type === 'notice'"
                class="agent-notice"
                :class="part.kind === 'error' ? 'agent-notice--error' : 'agent-notice--aborted'"
                text="xs"
              >
                <i
                  shrink="0"
                  :class="
                    part.kind === 'error' ? 'i-ri-error-warning-line' : 'i-ri-stop-circle-line'
                  "
                />
                <span>{{ part.text }}</span>
              </div>
            </template>
          </div>
        </div>
      </template>
    </div>

    <!-- 底栏渐隐：内容滚入输入岛下方时自下而上软透 -->
    <div class="chrome-fade-bottom" aria-hidden="true" />

    <div class="agent-footer" ref="footerRef">
      <!-- 滚底：非贴底即显；中止：仅输出中。
           零宽中心锚点；两钮 absolute + translate 定位（solo 居中 / pair 分居中线两侧） -->
      <Transition
        enter-active-class="agent-float-group-enter-active"
        leave-active-class="agent-float-group-leave-active"
        enter-from-class="agent-float-group-enter-from"
        leave-to-class="agent-float-group-leave-to"
      >
        <div v-if="showFloatActions" class="agent-float-actions">
          <Transition
            enter-active-class="agent-float-btn-enter-active"
            leave-active-class="agent-float-btn-leave-active"
            enter-from-class="agent-float-btn-enter-from"
            leave-to-class="agent-float-btn-leave-to"
          >
            <BaseButton
              v-if="showScrollBtn"
              key="scroll"
              class="agent-float-scroll"
              :class="isFloatPair ? 'is-pair-side' : 'is-solo'"
              icon="i-ri-arrow-down-s-line"
              :title="t('agent.scrollBottom')"
              :aria-label="t('agent.scrollBottom')"
              @click="scrollToBottom"
            />
          </Transition>
          <Transition
            enter-active-class="agent-float-btn-enter-active"
            leave-active-class="agent-float-btn-leave-active"
            enter-from-class="agent-float-btn-enter-from"
            leave-to-class="agent-float-btn-leave-to"
          >
            <BaseButton
              v-if="showStopBtn"
              key="stop"
              class="agent-float-stop"
              :class="isFloatPair ? 'is-pair-side' : 'is-solo'"
              icon="i-ri-stop-mini-fill"
              :title="t('agent.stopShortcut')"
              :aria-label="t('agent.stop')"
              @click="agent.abort()"
            />
          </Transition>
        </div>
      </Transition>

      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        rounded="panel"
        :placeholder="t('agent.placeholder')"
        :rows="1"
        :max-height="120"
        @submit="handleSubmit"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  ref,
  nextTick,
  onMounted,
  onUnmounted,
  onActivated,
  onDeactivated,
  watch,
} from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { isAgentProviderReady } from './config'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import AgentTextPart from './AgentTextPart.vue'
import AgentToolStep from './AgentToolStep.vue'
import AgentReasoningPart from './AgentReasoningPart.vue'
import { useAgentChat, focusInputTick } from './agent'
import { getMessageText, isStreamingText, streamLayoutKey, partKey } from './view-logic'
import './agent-step.css'

/** 距底部多少 px 内视为贴底（自动滚底生效） */
const NEAR_BOTTOM_PX = 24

const agent = useAgentChat()
const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const scrollRef = ref<HTMLElement>()
const footerRef = ref<HTMLElement>()
const inputText = ref('')
/** 用户是否贴底：仅贴底时 streaming 增量才自动滚底，上翻阅读时不打断 */
const stickToBottom = ref(true)

/**
 * 底部输入岛预留高度 = footer 实际高 + 底边距(--space)。
 * ResizeObserver 跟踪 textarea 自动撑高，使滚动区底 padding 与渐隐高度始终对齐 footer。
 */
const footerReserve = ref(60)

watch(footerRef, (el, _old, onCleanup) => {
  if (!el) return
  const measure = () => {
    footerReserve.value = el.offsetHeight + 12 // + --space 底边距
  }
  measure()
  const ro = new ResizeObserver(measure)
  ro.observe(el)
  onCleanup(() => ro.disconnect())
})

const displayMessages = computed(() => agent.messages.value)

/// KeepAlive deactivate（切到别的扩展）时不渲染消息 DOM，释放 WebKit 内存。
/// 数据常驻 messages 单例。
const isViewActive = ref(true)
const shouldRenderMessages = computed(() => displayMessages.value.length > 0 && isViewActive.value)

/** 有效选用可解析（含无显式选用时默认首个可用提供商）即可对话 */
const isConfigured = computed(() => isAgentProviderReady.value)

/** 滚底钮：有消息且非贴底；中止钮：仅输出中 */
const showScrollBtn = computed(() => displayMessages.value.length > 0 && !stickToBottom.value)
const showStopBtn = computed(() => agent.isGenerating.value)
const showFloatActions = computed(() => showScrollBtn.value || showStopBtn.value)
const isFloatPair = computed(() => showScrollBtn.value && showStopBtn.value)

/** 新会话清空后强制贴底 + 聚焦输入框 */
watch(
  () => displayMessages.value.length,
  (n) => {
    if (n === 0) {
      stickToBottom.value = true
      nextTick(() => textareaRef.value?.focus())
    }
  },
)

const appStore = useAppStore()

/**
 * 操作 accessory 后回归输入框（agent 主交互目标）：
 * - 选模型：Actions 监听 BaseSelect 的 focusout，relatedTarget 为空（Esc/选中/外点/toggle-off，
 *   含选原值）时自增 focusInputTick；Tab 切换 relatedTarget 非空不触发，保留主窗 Tab 环
 * - 从设置子视图返回（config → null；KeepAlive deactivate 下 watch 仍触发，nextTick 等 DOM 重插后 focus）
 */
watch(focusInputTick, () => nextTick(() => textareaRef.value?.focus()))
// 窗口获焦（Focused(true)）后聚焦输入框：mount 时窗口尚隐藏则跳过聚焦——
// 未激活应用内聚焦可编辑元素会触发 WebKit activateIgnoringOtherApps 抢走前台，
// 与「show 不抢 active」的设计相悖且干扰 frontmost 捕获时序
function onWinFocused() {
  if (appStore.activeExtId !== 'agent' || appStore.activeSubview || appStore.isDialogOpen) return
  textareaRef.value?.focus()
}
onMounted(() => window.addEventListener('window-focused', onWinFocused))
onUnmounted(() => window.removeEventListener('window-focused', onWinFocused))
watch(
  () => appStore.activeSubview,
  (sub, prev) => {
    if (prev === 'config' && !sub) nextTick(() => textareaRef.value?.focus())
  },
)

function isNearBottom(el: HTMLElement): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX
}

function onScroll() {
  const el = scrollRef.value
  if (!el) return
  stickToBottom.value = isNearBottom(el)
}

/** 用户点滚底：平滑滚动，贴底后由 onScroll 收 stickToBottom（按钮 leave 过渡） */
function scrollToBottom() {
  const el = scrollRef.value
  if (!el) return
  el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
}

/** streaming 贴底：瞬时到位，避免 smooth 拖尾 */
function scrollToBottomInstant() {
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
    if (stickToBottom.value) scrollToBottomInstant()
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
  inputText.value = ''
  // 发送后跟读最新输出
  stickToBottom.value = true
  await agent.sendMessage(text)
  await nextTick()
  scrollToBottomInstant()
  nextTick(() => textareaRef.value?.focus())
}

/// 拦截 markdown-body 内的链接点击：用系统浏览器打开（不在 webview 内导航）
async function onContentClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  const anchor = target?.closest?.('a') as HTMLAnchorElement | null
  if (!anchor) return
  const href = anchor.getAttribute('href') || ''
  // 仅拦截 http/https（mailto/tel 等让浏览器默认处理）
  if (!/^https?:\/\//i.test(href)) return
  e.preventDefault()
  e.stopPropagation()
  try {
    await open(href)
  } catch (err) {
    console.warn('Failed to open URL:', err)
  }
}

// 窗口隐藏（未 key）时跳过 mount 聚焦（见 onWinFocused 注释），由窗口获焦事件补聚焦
onMounted(() =>
  nextTick(() => {
    if (document.hasFocus()) textareaRef.value?.focus()
  }),
)

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
onActivated(() => {
  isViewActive.value = true
})
onDeactivated(() => {
  isViewActive.value = false
})
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

/* 未配置：空态居中 */
.agent-setup {
  flex: 1 1 0%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 0;
  padding: var(--chrome-fade-height) var(--space) var(--space);
}

/* 空态：在搜索栏和悬浮输入之间居中；底预留输入岛避免重叠 */
.agent-empty {
  flex: 1 1 0%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
  padding: var(--chrome-fade-height) var(--space) var(--agent-footer-reserve);
}

/*
 * 消息滚动区：铺满 layout，底被悬浮输入盖住
 * - 顶 padding = chrome-fade，消息可滚入搜索栏下层
 * - 底 padding = footer 预留 + 消息间距（末条距输入岛恒 --space，与消息间距一致）
 */
.agent-scroll {
  flex: 1 1 0%;
  display: flex;
  flex-direction: column;
  gap: var(--space);
  min-height: 0;
  overflow-y: auto;
  padding: var(--chrome-fade-height) var(--space) calc(var(--agent-footer-reserve) + var(--space));
  /* scrollIntoView（block:start）对齐到 padding-top 内侧，避免消息被搜索栏 chrome-fade 遮挡 */
  scroll-padding-top: var(--chrome-fade-height);
}

/* 消息行：用户右 / 助手左 */
.agent-row {
  display: flex;
  width: 100%;
  min-width: 0;
}

.agent-row--user {
  justify-content: flex-end;
}

.agent-row--assistant {
  justify-content: flex-start;
}

/* 用户气泡：accent 浅染实底（--color-bubble）；描边同系 accent-line-soft */
.agent-bubble--user {
  max-width: 85%;
  padding: 10px var(--space);
  border: 1px solid var(--accent-line-soft);
  border-radius: var(--radius-panel);
  background: var(--color-bubble);
  color: var(--color-text-primary);
  font-size: 14px;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
}

/* 助手卡：布局；面/抬升 = soft-card（与 system-status 等同） */
.agent-card {
  max-width: 100%;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space);
  min-width: 0;
  padding: var(--space);
}

/* 错误 / 中止 notice（不进 LLM）；内垫与 step 同 --space-soft */
.agent-notice {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: var(--space-soft);
  border-radius: var(--radius-ctrl);
  line-height: 1.5;
  word-break: break-word;
}

.agent-notice i {
  margin-top: 1px;
  font-size: 14px;
}

.agent-notice--error {
  color: var(--color-danger);
  background: var(--color-danger-soft);
}

.agent-notice--aborted {
  color: var(--color-text-muted);
  background: var(--color-mist);
}

/* 底部渐隐：高度跟随输入岛（覆盖全局 --chrome-fade-bottom-height），渐隐精确贴合 footer 区域 */
.chrome-fade-bottom {
  height: var(--agent-footer-reserve);
}

/* 底部输入岛：absolute；边距 --space；抬升 --shadow-bar */
.agent-footer {
  position: absolute;
  inset-inline: var(--space);
  bottom: var(--space);
  z-index: 10;
  flex: none;
}
.agent-footer :deep(.ui-field) {
  box-shadow: var(--shadow-bar);
}
.agent-footer :deep(.ui-field:focus-within) {
  box-shadow: var(--shadow-bar) !important;
}

/*
 * 输入框上方：零宽锚点钉在水平中线（left:50%, width:0）
 * 两钮均 absolute，只改 translate → solo/pair 切换可插值，中止消失后滚底能滑回正中
 */
.agent-float-actions {
  position: absolute;
  bottom: 100%;
  left: 50%;
  width: 0;
  height: 30px;
  margin-bottom: 8px;
  z-index: 10;
}

.agent-float-group-enter-active,
.agent-float-group-leave-active {
  transition: opacity var(--duration-normal) var(--ease-out);
}
.agent-float-group-leave-active {
  transition: opacity var(--duration-fast) var(--ease-in);
}
.agent-float-group-enter-from,
.agent-float-group-leave-to {
  opacity: 0;
}
.agent-float-btn-enter-active,
.agent-float-btn-leave-active {
  transition:
    opacity var(--duration-normal) var(--ease-out),
    scale var(--duration-normal) var(--ease-out),
    translate var(--duration-normal) var(--ease-out);
}
.agent-float-btn-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-in),
    scale var(--duration-fast) var(--ease-in),
    translate var(--duration-fast) var(--ease-in);
}
.agent-float-btn-enter-from,
.agent-float-btn-leave-to {
  opacity: 0;
  scale: 0.9;
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
 * 浮钮：soft-surface 面 + --shadow-float*；!important 压 soft-chip。
 * 中止 aurora 用多层 background（WK 裁 ::before 失败）。
 */
.agent-float-actions :deep(.agent-float-scroll),
.agent-float-actions :deep(.agent-float-stop) {
  position: absolute;
  top: 0;
  left: 0;
  box-sizing: border-box;
  height: 30px;
  border: 1px solid var(--soft-surface-border);
  border-radius: 999px;
  background: var(--soft-surface-fill);
  backdrop-filter: blur(var(--soft-surface-blur)) saturate(var(--soft-surface-saturate));
  -webkit-backdrop-filter: blur(var(--soft-surface-blur)) saturate(var(--soft-surface-saturate));
  box-shadow: var(--shadow-float) !important;
  transition:
    translate var(--duration-normal) var(--ease-out),
    box-shadow 220ms var(--ease-out),
    border-color 220ms var(--ease-out),
    background-color 220ms var(--ease-out) !important;
}
.agent-float-actions :deep(.agent-float-scroll:hover:not(:disabled)),
.agent-float-actions :deep(.agent-float-stop:hover:not(:disabled)) {
  border-color: var(--soft-surface-border) !important;
  box-shadow: var(--shadow-float-hover) !important;
}
.agent-float-actions :deep(.agent-float-scroll:hover:not(:disabled)),
.agent-float-actions :deep(.agent-float-scroll:active:not(:disabled)) {
  background: var(--soft-surface-fill) !important;
}
.agent-float-actions :deep(.agent-float-scroll:active:not(:disabled)),
.agent-float-actions :deep(.agent-float-stop:active:not(:disabled)) {
  border-color: var(--soft-surface-border) !important;
  box-shadow: var(--shadow-float-active) !important;
}

/* 滚底：圆。solo 居中；pair 整颗在中线左侧（右缘 = 中线 - 4px 半槽） */
.agent-float-actions :deep(.agent-float-scroll) {
  width: 30px;
  min-width: 30px;
  padding: 0;
}
.agent-float-actions :deep(.agent-float-scroll.is-solo) {
  translate: -50% 0;
}
.agent-float-actions :deep(.agent-float-scroll.is-pair-side) {
  translate: calc(-100% - 4px) 0;
}

/* 中止：solo 居中；pair 左缘 = 中线 + 4px 半槽 */
.agent-float-actions :deep(.agent-float-stop.is-solo) {
  translate: -50% 0;
}
.agent-float-actions :deep(.agent-float-stop.is-pair-side) {
  translate: 4px 0;
}

/* 中止：胶囊 + aurora（--agent-aurora-*）；锁边/底压 soft-chip */
.agent-float-actions :deep(.agent-float-stop),
.agent-float-actions :deep(.agent-float-stop:hover:not(:disabled)),
.agent-float-actions :deep(.agent-float-stop:active:not(:disabled)) {
  --agent-aurora-base: color-mix(in srgb, var(--color-card-solid) 96%, transparent);
  width: auto;
  min-width: 48px;
  padding: 0 14px;
  border-color: var(--soft-surface-border) !important;
  background:
    radial-gradient(
      circle at var(--ax) var(--ay),
      color-mix(in srgb, var(--color-accent) 28%, transparent) 0%,
      transparent 55%
    ),
    radial-gradient(
      circle at var(--bx) var(--by),
      color-mix(in srgb, var(--color-accent) 16%, var(--color-card-solid)) 0%,
      transparent 50%
    ),
    radial-gradient(
      circle at var(--cx) var(--cy),
      color-mix(in srgb, var(--agent-aurora-warm) 32%, transparent) 0%,
      transparent 52%
    ),
    conic-gradient(
      from var(--agent-ai-angle),
      transparent 0deg 170deg,
      color-mix(in srgb, var(--color-accent) 22%, var(--color-card-solid)) 210deg,
      color-mix(in srgb, var(--agent-aurora-warm-hi) 26%, var(--color-card-solid)) 255deg,
      color-mix(in srgb, var(--color-accent) 14%, transparent) 300deg,
      transparent 340deg 360deg
    ),
    var(--agent-aurora-base) !important;
  background-origin: padding-box;
  background-clip: padding-box;
  animation:
    agent-ai-rotate 5.5s linear infinite,
    agent-ai-aurora 3.8s ease-in-out infinite;
}

/* hover：仅压暗底，光层复用 */
.agent-float-actions :deep(.agent-float-stop:hover) {
  --agent-aurora-base: color-mix(in srgb, var(--color-card-solid) 92%, transparent);
}

.agent-float-actions :deep(.agent-float-scroll i),
.agent-float-actions :deep(.agent-float-stop i) {
  position: relative;
  font-size: 14px;
}

/* 中止图标：实心主色字（非 danger 红） */
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
</style>
