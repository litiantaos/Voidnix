<template>
  <div p="4" h="full" overflow="y-auto">
    <div class="prose prose-sm" text="tx-primary" max-w="none" v-html="renderedContent" />
    <div v-if="view.actions?.length" m="t-4" class="action-footer">
      <button
        v-for="a in view.actions"
        :key="a.id"
        text="sm"
        class="ui-ctrl"
        p="x-3 y-1.5"
        rounded="md"
        :class="
          a.destructive
            ? 'text-red-500 bg-red-50'
            : a.primary
              ? 'bg-accent text-white'
              : 'text-tx-subtle'
        "
        @click="emit('action', a.id, {})"
      >
        {{ a.title }}
        <span v-if="a.shortcut" text="xs" m="l-1" opacity="60">{{ a.shortcut }}</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { MarkdownView } from '@/types/declarative'

const props = defineProps<{ view: MarkdownView }>()

const emit = defineEmits<{
  action: [actionId: string, payload: Record<string, unknown>]
}>()

const renderedContent = computed(() => {
  // Simple markdown rendering (bold, italic, code blocks, links)
  return props.view.content
    .replace(/</g, '&lt;')
    .replace(
      /```(\w*)\n([\s\S]*?)```/g,
      '<pre class="bg-tx-faint/10 rounded-md p-3 my-2 overflow-x-auto text-xs"><code>$2</code></pre>',
    )
    .replace(/`([^`]+)`/g, '<code class="bg-tx-faint/10 rounded px-1 text-xs">$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/^### (.+)$/gm, '<h3 class="text-sm font-semibold mt-3 mb-1">$1</h3>')
    .replace(/^## (.+)$/gm, '<h2 class="text-base font-semibold mt-4 mb-2">$1</h2>')
    .replace(/^# (.+)$/gm, '<h1 class="text-lg font-bold mt-5 mb-2">$1</h1>')
    .replace(/^- (.+)$/gm, '<li class="ml-4 text-sm">$1</li>')
    .replace(/\n/g, '<br>')
})
</script>
