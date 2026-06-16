<template>
  <div
    h="7"
    border="t black/5"
    px="5"
    flex="~"
    shrink="0"
    items="center"
    justify="between"
    text="xs"
  >
    <!-- 左：瞬时消息 / 结果计数 -->
    <div role="status" aria-live="polite" min-w="0" flex="~" items="center">
      <Transition
        mode="out-in"
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="opacity-0"
        leave-active-class="transition duration-100 ease-in"
        leave-to-class="opacity-0"
      >
        <div v-if="appStore.statusMessage" key="msg" flex gap="1" items="center">
          <i class="i-ri-check-line text-accent" text="tx-muted" />
          <span text="tx-muted">{{ appStore.statusMessage }}</span>
        </div>
        <span v-else-if="isSearchMode && isLoading" key="loading" text="tx-hint">正在搜索…</span>
        <span v-else-if="isSearchMode && resultCount > 0" key="count" text="tx-hint">
          {{ resultCount }} 项
        </span>
        <span v-else key="empty"></span>
      </Transition>
    </div>

    <!-- 右：上下文快捷键提示 -->
    <div flex="~ none" gap="3" items="center" text="tx-hint">
      <span v-for="(hint, i) in hints" :key="i" flex gap="1" items="center">
        <kbd
          text="xs"
          font="medium mono"
          leading="none"
          rounded
          bg="black/5"
          flex
          h="4"
          px="1"
          items="center"
          justify="center"
          >{{ hint.keys.join('+') }}</kbd
        >
        <span>{{ hint.label }}</span>
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { getModule } from '@/core/module-registry'
import type { SearchResult } from '@/types/module'

interface ShortcutHint {
  keys: string[]
  label: string
}

const props = defineProps<{
  resultCount: number
  selectedResult?: SearchResult
  isLoading?: boolean
}>()

const appStore = useAppStore()

const activeModule = computed(() => {
  const id = appStore.activeModuleId
  return id ? getModule(id) : null
})

const hasQuery = computed(() => appStore.searchQuery.trim().length > 0)
const isSearchMode = computed(() => !appStore.activeModuleId && !appStore.activeSubview)

const hints = computed<ShortcutHint[]>(() => {
  if (appStore.activeSubview) {
    return [{ keys: ['esc'], label: '返回' }]
  }

  const mod = activeModule.value

  if (mod) {
    const result: ShortcutHint[] = []
    const inputActive = !mod.disableSearchInput

    if (inputActive && mod.enterHint) {
      result.push({ keys: ['enter'], label: mod.enterHint })
    }

    if (mod.multiSelectHint) {
      result.push({ keys: ['shift/cmd'], label: '多选' })
    }

    if (mod.deleteHint) {
      result.push({ keys: ['cmd', '⌫'], label: mod.deleteHint })
    }

    result.push({ keys: ['esc'], label: hasQuery.value && inputActive ? '清空' : '返回' })
    return result
  }

  const result: ShortcutHint[] = []
  if (props.resultCount > 0) {
    result.push({ keys: ['enter'], label: '打开' })
    if (props.selectedResult?.data?.path) {
      result.push({ keys: ['cmd', 'enter'], label: '访达' })
    }
  }
  result.push({ keys: ['esc'], label: hasQuery.value ? '清空' : '关闭' })
  return result
})
</script>
