<template>
  <!-- 设置：齿轮 toggle，激活态 fill 图标（proxy/agent 同款） -->
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    :title="t('systemStatus.group.settings')"
    :aria-label="t('systemStatus.group.settings')"
    @click="toggleConfig"
  />
</template>

<script setup lang="ts">
import BaseButton from '@/components/ui/BaseButton.vue'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'

/// 搜索栏配件：设置入口，toggle config 子视图。
const appStore = useAppStore()

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
