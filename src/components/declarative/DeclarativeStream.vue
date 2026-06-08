<template>
  <div flex="~ col" h="full">
    <div p="4" flex="1" overflow="y-auto" space="y-3">
      <div
        v-for="block in view.blocks"
        :key="block.id"
        flex
        :class="block.role === 'user' ? 'justify-end' : 'justify-start'"
      >
        <div
          text="sm"
          p="x-3 y-2"
          rounded="lg"
          max-w="[80%]"
          :class="block.role === 'user' ? 'bg-accent text-white' : 'bg-tx-faint/10 text-tx-primary'"
        >
          <div v-if="block.metadata?.timestamp" text="xs tx-faint" m="b-1">
            {{
              new Date(block.metadata.timestamp).toLocaleTimeString('zh-CN', {
                hour: '2-digit',
                minute: '2-digit',
              })
            }}
          </div>
          <div v-html="renderContent(block.content)" class="prose-sm" />
          <div
            v-if="!block.done"
            m="l-0.5"
            class="align-middle animate-pulse"
            rounded="sm"
            bg="accent"
            h="4"
            w="2"
            inline-block
          />
        </div>
      </div>
    </div>
    <div v-if="view.input" p="3" border="t tx-faint/20">
      <div flex gap="2">
        <BaseInput
          v-if="view.input.type === 'text'"
          v-model="inputText"
          :placeholder="view.input.placeholder || '输入消息...'"
          class="flex-1"
          @keyup.enter="sendMessage"
        />
        <BaseTextarea
          v-else-if="view.input.type === 'textarea'"
          v-model="inputText"
          placeholder="输入消息..."
          class="flex-1"
        />
        <button
          text="sm white"
          class="ui-ctrl"
          p="x-4 y-1.5"
          rounded="md"
          bg="accent"
          @click="sendMessage"
        >
          发送
        </button>
      </div>
    </div>
    <div v-if="view.actions?.length" p="x-4 y-2" border="t tx-faint/20" flex gap="2">
      <button
        v-for="a in view.actions"
        :key="a.id"
        text="xs"
        class="ui-ctrl"
        p="x-2 y-1"
        rounded="md"
        :class="a.destructive ? 'text-red-500 bg-red-50' : 'text-tx-subtle'"
        @click="emit('action', a.id, {})"
      >
        {{ a.title }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { StreamView } from '@/types/declarative'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'

defineProps<{ view: StreamView }>()

const emit = defineEmits<{
  action: [actionId: string, payload: Record<string, unknown>]
}>()

const inputText = ref('')

function sendMessage() {
  if (!inputText.value.trim()) return
  emit('action', 'send', { text: inputText.value })
  inputText.value = ''
}

function renderContent(content: string): string {
  return content
    .replace(/</g, '&lt;')
    .replace(
      /```(\w*)\n([\s\S]*?)```/g,
      '<pre class="bg-black/5 rounded-md p-2 my-1 overflow-x-auto text-xs"><code>$2</code></pre>',
    )
    .replace(/`([^`]+)`/g, '<code class="bg-black/5 rounded px-1 text-xs">$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\n/g, '<br>')
}
</script>
