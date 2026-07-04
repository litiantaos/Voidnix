<template>
  <template v-for="item in items" :key="item.id">
    <BaseButton
      :variant="appStore.activeSubview === item.id ? 'primary' : 'default'"
      @click="toggle(item.id)"
      >{{ item.label }}</BaseButton
    >
  </template>
</template>

<script setup lang="ts">
import BaseButton from '@/components/ui/BaseButton.vue'
import { useAppStore } from '@/stores/app'

/// 诊断入口（连接/规则/日志）：文字按钮 toggle 子视图。
const appStore = useAppStore()

const items = [
  { id: 'connections', label: '连接' },
  { id: 'rules', label: '规则' },
  { id: 'logs', label: '日志' },
] as const

function toggle(id: string) {
  if (appStore.activeSubview === id) appStore.closeSubview()
  else appStore.openSubview(id)
  // 切换视图清空搜索（每个视图独立搜索语义）
  appStore.setSearchQuery('')
}
</script>
