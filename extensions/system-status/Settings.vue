<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="settingsItems" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'
import { config } from './config'

/// 系统状态设置子视图（config subview）：菜单栏状态段显隐开关。
const settingsItems = computed<SettingItem[]>(() => [
  {
    id: 'menubarVisible',
    title: t('systemStatus.settings.menubarToggle'),
    type: 'toggle',
    value: config.menubarVisible,
    update: (visible: boolean | string | number) => {
      config.menubarVisible = visible as boolean
    },
    group: t('systemStatus.group.settings'),
  },
])
</script>
