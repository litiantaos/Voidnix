<template>
  <div class="flex-col-view">
    <BaseSettingsList :items="items" shortcut-id="screenshot" @execute="onExecute">
      <template #trailing-savePath>
        <BaseButton @click.stop="pickSavePath">{{ t('screenshot.select') }}</BaseButton>
      </template>
    </BaseSettingsList>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { withSuppressBlur } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { config as screenshotConfig } from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const { value: screenshotShortcutValue, update: handleShortcutChange } = useShortcutConfig(
  'screenshot',
  'Alt+S',
)

async function pickSavePath() {
  const selected = await withSuppressBlur(() => invoke<string>(CMD.pickDirectory))
  if (selected) {
    screenshotConfig.savePath = selected
  }
}

function savePathDisplay(path: string): string {
  if (!path) return '~/Downloads'
  return path.replace(/^\/Users\/[^/]+/, '~')
}

function onExecute(item: SettingItem) {
  if (item.id === 'savePath') void pickSavePath()
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'screenshot-shortcut',
    title: t('settings.shortcut'),
    type: 'shortcut',
    value: screenshotShortcutValue.value,
    update: handleShortcutChange,
    group: t('common.group.general'),
  },
  {
    id: 'savePath',
    title: t('screenshot.savePath'),
    subtitle: savePathDisplay(screenshotConfig.savePath),
    type: 'custom',
    group: t('common.group.general'),
  },
])
</script>
