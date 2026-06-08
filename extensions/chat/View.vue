<template>
  <BaseEmptyState v-if="!isConfigured" icon="i-ri-settings-3-line" title="请先配置 AI API" />

  <div v-else p="x-5 t-5" flex="~ col" h="full" min-h="0">
    <div v-if="displayMessages.length > 0" flex="~ 1 col" gap="3" min-h="0" overflow="y-auto">
      <div
        v-for="(msg, index) in displayMessages"
        :key="index"
        text="sm tx-primary"
        leading="relaxed"
        p="l-3"
        border="l-3"
        w="full"
        :class="
          msg.role === 'user'
            ? 'border-l-blue-500 whitespace-pre-wrap break-all overflow-hidden'
            : 'border-l-gray-300'
        "
      >
        <template v-if="msg.role === 'user'">{{ msg.content }}</template>
        <div v-else class="markdown-body" v-html="renderMarkdown(msg.content)" />
      </div>
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
        <p text="xs tx-muted">日常问题、工作任务...</p>
      </div>

      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        placeholder="聊点什么..."
        class="bg-[#f0f0f0] pointer-events-auto"
        @submit="handleSubmit"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, onMounted } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { currentConversation, streamingMessage, sendMessage, isGenerating } from './index'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'

marked.setOptions({
  gfm: true,
  breaks: true,
})

const renderMarkdown = (content: string) => {
  const result = marked.parse(content)
  return typeof result === 'string' ? DOMPurify.sanitize(result) : ''
}

const MAX_INPUT_LENGTH = 8192

const settings = useSettingsStore()
const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const inputText = ref('')

const streamingMessageObj = computed(() => {
  if (!streamingMessage.value) return null
  return { role: 'assistant' as const, content: streamingMessage.value }
})

const displayMessages = computed(() => {
  const messages = [...currentConversation.value.messages]
  if (streamingMessageObj.value) {
    messages.push(streamingMessageObj.value)
  }
  return messages
})

const isConfigured = computed(
  () => settings.activeChatConfig.endpoint && settings.activeChatConfig.apiKey,
)

async function handleSubmit() {
  const text = inputText.value.trim()
  if (!text || isGenerating.value) return
  if (text.length > MAX_INPUT_LENGTH) inputText.value = text.slice(0, MAX_INPUT_LENGTH)
  inputText.value = ''
  await sendMessage(text)
  nextTick(() => textareaRef.value?.focus())
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

.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin-bottom: 0.75em;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid rgba(0, 0, 0, 0.1);
  padding: 0.5em 0.75em;
  text-align: left;
}

.markdown-body :deep(th) {
  background: rgba(0, 0, 0, 0.04);
  font-weight: 600;
}

.markdown-body :deep(a) {
  color: var(--color-accent);
  text-decoration: none;
}

.markdown-body :deep(a:hover) {
  text-decoration: underline;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.1);
  margin: 1em 0;
}

.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: 6px;
}

.markdown-body :deep(*:last-child) {
  margin-bottom: 0;
}
</style>
