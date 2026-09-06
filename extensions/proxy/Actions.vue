<template>
  <template v-for="item in items" :key="item.id">
    <BaseButton
      :variant="appStore.activeSubview === item.id ? 'primary' : 'default'"
      @click="toggle(item.id)"
      >{{ item.label }}</BaseButton
    >
  </template>
  <!-- 设置（完全卸载入口）：齿轮 toggle，激活态 fill 图标 -->
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    :title="t('proxy.openSettings')"
    @click="toggle('config')"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'

/// 诊断入口（连接/规则/日志）+ 设置（完全卸载）：toggle 子视图。
const appStore = useAppStore()

const items = computed(() => [
  { id: 'connections', label: t('proxy.connections') },
  { id: 'rules', label: t('proxy.rules') },
  { id: 'logs', label: t('proxy.logs') },
])

function toggle(id: string) {
  if (appStore.activeSubview === id) appStore.closeSubview()
  else appStore.openSubview(id)
  // 切换视图清空搜索（每个视图独立搜索语义）
  appStore.setSearchQuery('')
}
</script>
