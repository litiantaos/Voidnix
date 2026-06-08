<template>
  <div flex h="full">
    <div p="4" flex="1" overflow="y-auto">
      <div class="prose prose-sm" text="tx-primary" max-w="none" v-html="renderedContent" />
    </div>
    <div
      v-if="view.metadata?.length"
      p="4"
      border="l tx-faint/20"
      shrink="0"
      w="48"
      overflow="y-auto"
      space="y-3"
    >
      <template v-for="(m, i) in view.metadata" :key="i">
        <div v-if="m.type === 'label'" text="xs">
          <div text="tx-faint">{{ m.title }}</div>
          <div text="tx-primary" m="t-0.5">{{ m.text }}</div>
        </div>
        <div v-else-if="m.type === 'link'" text="xs">
          <div text="tx-faint">{{ m.title }}</div>
          <a :href="m.url" text="accent" m="t-0.5" underline block target="_blank">{{
            m.text || m.url
          }}</a>
        </div>
        <div v-else-if="m.type === 'tag'" text="xs">
          <div text="tx-faint" m="b-1">{{ m.title }}</div>
          <div flex flex-wrap gap="1">
            <span
              v-for="t in m.tags"
              :key="t.text"
              text="xs"
              p="x-1.5 y-0.5"
              rounded
              bg="tx-faint/10"
              >{{ t.text }}</span
            >
          </div>
        </div>
        <hr v-else-if="m.type === 'separator'" border="tx-faint/10" />
      </template>
    </div>
  </div>
  <div v-if="view.actions?.length" p="3" class="action-footer">
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
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { DetailView } from '@/types/declarative'

const props = defineProps<{ view: DetailView }>()

const emit = defineEmits<{
  action: [actionId: string, payload: Record<string, unknown>]
}>()

const renderedContent = computed(() => {
  return props.view.markdown
    .replace(/</g, '&lt;')
    .replace(
      /```(\w*)\n([\s\S]*?)```/g,
      '<pre class="bg-tx-faint/10 rounded-md p-3 my-2 overflow-x-auto text-xs"><code>$2</code></pre>',
    )
    .replace(/`([^`]+)`/g, '<code class="bg-tx-faint/10 rounded px-1 text-xs">$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\n/g, '<br>')
})
</script>
