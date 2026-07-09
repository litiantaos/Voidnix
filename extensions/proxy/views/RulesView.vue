<template>
  <div flex="~ col">
    <BaseList :items="filtered" v-model:selected-index="selectedIndex">
      <template #item="{ item }">
        <div flex p="3" gap="3" text="xs" select="none">
          <span shrink="0" w="28" whitespace="nowrap" text="muted">{{ item.type }}</span>
          <span
            shrink="0"
            w="28"
            truncate
            :class="item.proxy === 'DIRECT' ? 'text-green-500' : 'text-accent'"
            >{{ item.proxy }}</span
          >
          <span flex="1" min-w="0" truncate :title="item.payload">{{ item.payload || '—' }}</span>
        </div>
      </template>
    </BaseList>
    <BaseEmptyState
      v-if="filtered.length === 0"
      icon="i-ri-ruler-line"
      :loading="loading"
      :title="loading ? '加载中…' : '无规则'"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

interface Rule {
  type: string
  payload: string
  proxy: string
}

const appStore = useAppStore()
const rules = ref<Rule[]>([])
const loading = ref(true)
const selectedIndex = ref(0)

const filtered = computed(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return rules.value
  return rules.value.filter(
    (r) =>
      r.type.toLowerCase().includes(q) ||
      r.payload.toLowerCase().includes(q) ||
      r.proxy.toLowerCase().includes(q),
  )
})

onMounted(async () => {
  try {
    const resp = await invoke<{ rules: Rule[] }>(CMD.proxyGetRules)
    rules.value = resp.rules ?? []
  } catch {
    /* 静默：mihomo 未运行时后端返回空，极端 IPC 错误忽略 */
  } finally {
    loading.value = false
  }
})
</script>
