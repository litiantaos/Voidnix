<template>
  <BaseSelect
    v-if="modelOptions.length > 0"
    :model-value="effectiveProviderModelKey"
    :options="modelOptions"
    class="max-w-64"
    @update:model-value="handleModelChange"
    @focusout="onSelectFocusOut"
  />
  <div v-if="userMessages.length > 0" ref="historyWrapRef" class="inline-flex">
    <BaseButton
      icon="i-ri-chat-history-line"
      title="历史消息"
      aria-label="历史消息"
      aria-haspopup="listbox"
      :aria-expanded="isHistoryOpen"
      @click.stop="toggleHistory"
    />
  </div>
  <BaseButton
    v-if="userMessages.length > 0"
    icon="i-ri-add-line"
    title="新会话"
    aria-label="新会话"
    @click="handleNewConversation"
  />
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    :title="appStore.activeSubview === 'config' ? '关闭设置' : '设置'"
    :aria-label="appStore.activeSubview === 'config' ? '关闭设置' : '设置'"
    @click="toggleConfig"
  />

  <!-- 历史消息浮层：列出本会话所有 user 消息，点击跳转到对应位置 -->
  <Teleport to="body">
    <Transition :css="false" @enter="historyOnEnter" @leave="historyOnLeave">
      <div
        v-if="isHistoryOpen"
        ref="historyDropdownRef"
        class="agent-history-panel dropdown-panel"
        role="listbox"
        aria-label="历史消息"
        tabindex="-1"
        @keydown="onHistoryKeydown"
        @focusout="onHistoryFocusOut"
      >
        <BaseDropdownItems
          :items="historyItems"
          :active-index="historyIndex"
          @select="onHistorySelect"
          @hover="(i: number) => (historyIndex = i)"
        />
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, onMounted, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { useAgentChat, focusInputTick } from './agent'
import { setProviderModelKey, modelSelectOptions, effectiveProviderModelKey } from './config'
import { getMessageText, buildHistoryLabel } from './view-logic'
import type { AgentMessage } from '@/types/agent'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import BaseDropdownItems, { type PanelItem } from '@/components/ui/BaseDropdownItems.vue'
import { useFloating } from '@/composables/useFloating'
import { wrapIndex } from '@/utils/dom'

/** 历史浮层单行 label 截断上限（避免长消息撑爆行高） */
const HISTORY_LABEL_MAX = 60

const appStore = useAppStore()
const agent = useAgentChat()

const modelOptions = computed(() => modelSelectOptions())

/// 历史消息浮层：列出本会话所有 user 消息（按时间顺序），点击跳转到对应 DOM 位置
const historyWrapRef = ref<HTMLElement | null>(null)
const historyDropdownRef = ref<HTMLElement | null>(null)
const isHistoryOpen = ref(false)
const historyIndex = ref(-1)

const userMessages = computed(() =>
  agent.messages.value
    .filter((m): m is AgentMessage => m.role === 'user')
    .map((m) => ({ id: m.id, text: getMessageText(m) })),
)

const historyItems = computed<PanelItem[]>(() =>
  userMessages.value.map((m, i) => ({
    type: 'item',
    key: m.id,
    label: buildHistoryLabel(m.text, i + 1, HISTORY_LABEL_MAX),
  })),
)

const { onEnter: historyOnEnter, onLeave: historyOnLeave } = useFloating(
  historyWrapRef,
  historyDropdownRef,
  {
    isOpen: isHistoryOpen,
    placement: 'bottom-end',
    offset: 6,
    padding: 12,
  },
)

function openHistory() {
  historyIndex.value = userMessages.value.length - 1
  isHistoryOpen.value = true
  nextTick(() => historyDropdownRef.value?.focus())
}

function closeHistory() {
  isHistoryOpen.value = false
}

function toggleHistory() {
  if (isHistoryOpen.value) closeHistory()
  else openHistory()
}

function onHistorySelect(i: number) {
  const entry = userMessages.value[i]
  if (!entry) return
  closeHistory()
  jumpToMessage(entry.id)
}

/**
 * 滚动到指定 user 消息：scrollIntoView 自动滚最近可滚动祖先（View.vue 的消息区）。
 * View.vue 的 onScroll 会因 scroll 事件触发而更新 stickToBottom（block:start 不在贴底阈值内 → 自动 false），
 * 故无需跨组件协调贴底状态。
 */
function jumpToMessage(id: string) {
  const el = document.querySelector(`[data-msg-id="${CSS.escape(id)}"]`)
  el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function onHistoryKeydown(e: KeyboardEvent) {
  if (!isHistoryOpen.value) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    closeHistory()
    historyWrapRef.value?.focus()
    return
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    e.stopPropagation()
    const len = userMessages.value.length
    if (len === 0) return
    const cur = historyIndex.value < 0 ? len - 1 : historyIndex.value
    historyIndex.value = wrapIndex(cur, len, e.key === 'ArrowDown' ? 'down' : 'up')
    return
  }
  if (e.key === 'Enter') {
    e.preventDefault()
    e.stopPropagation()
    if (historyIndex.value >= 0) onHistorySelect(historyIndex.value)
  }
}

/**
 * 焦点离开浮层（含主窗 Tab 环 cycleFocus 切到邻钮）：关下拉。
 * 与 BaseSelect 同模式：relatedTarget 仍在浮层 / 触发按钮内则保留；否则关闭。
 * Tab 由 MainView 的 capture onTabKeydown 拦截 cycleFocus，本组件用 focusout 感知切焦并收起。
 */
function onHistoryFocusOut(e: FocusEvent) {
  if (!isHistoryOpen.value) return
  const next = e.relatedTarget as Node | null
  if (next && historyDropdownRef.value?.contains(next)) return
  if (next && historyWrapRef.value?.contains(next)) return
  closeHistory()
}

function onHistoryClickOutside(e: MouseEvent) {
  if (!isHistoryOpen.value) return
  const t = e.target as Node | null
  if (historyWrapRef.value?.contains(t)) return
  if (historyDropdownRef.value?.contains(t)) return
  closeHistory()
}

function handleModelChange(val: string | number) {
  setProviderModelKey(String(val))
}

/**
 * BaseSelect 焦点离开时：relatedTarget 为空 = Esc/选中/外点/toggle-off 主动关闭（焦点落 body），
 * 回归输入框；Tab 切换 relatedTarget 是 Tab 环内下一按钮（非空），不干预（主窗 cycleFocus 自管）。
 * 用 focusout 而非侵入 BaseSelect 加 close emit——焦点回归是 agent 自身交互契约。
 */
function onSelectFocusOut(e: FocusEvent) {
  if (!e.relatedTarget) focusInputTick.value++
}

function handleNewConversation() {
  agent.newConversation()
}

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}

onMounted(() => {
  document.addEventListener('mousedown', onHistoryClickOutside)
})
onUnmounted(() => {
  document.removeEventListener('mousedown', onHistoryClickOutside)
})
</script>

<style scoped>
/* 历史浮层：dropdown-panel 已自带面/阴影/圆角；这里只限宽与最大高度 */
.agent-history-panel {
  width: min(420px, 92vw);
  max-height: min(340px, 60vh);
  overflow-y: auto;
}
</style>
