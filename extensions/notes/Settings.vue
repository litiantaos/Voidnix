<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items" shortcut-id="notes" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import { t } from '@/runtime/i18n'
import { config } from './config'

const appStore = useAppStore()

const { value: shortcutValue, update } = useShortcutConfig('notes', 'Alt+N')
const handleShortcutChange = (val: string | number) => update(String(val))

const handleClear = async () => {
  const confirmed = await appStore.showConfirm({
    title: t('notes.clearTitle'),
    message: t('notes.clearMessage'),
    okLabel: t('common.confirm'),
    cancelLabel: t('common.cancel'),
  })
  // 置空 config 触发 View 侧 watch 以 applyText('') 走统一清空动画
  if (confirmed) config.content = ''
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'notes-shortcut',
    title: t('notes.settings.shortcut'),
    type: 'shortcut',
    group: t('notes.settings.groupGeneral'),
    value: shortcutValue.value,
    update: handleShortcutChange,
  },
  {
    id: 'notes-clear',
    title: t('notes.settings.clear'),
    type: 'button',
    variant: 'danger',
    label: t('notes.settings.clearAction'),
    group: t('notes.settings.groupData'),
    action: () => void handleClear(),
  },
])
</script>
