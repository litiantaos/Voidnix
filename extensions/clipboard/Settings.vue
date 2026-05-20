<template>
  <div class="pb-4 flex flex-col h-full">
    <BaseList
      :items="items"
      v-model:selected-index="selectedIndex"
      keyboard-navigation
      @execute="(item: SettingItem, _i: number, e?: KeyboardEvent) => handleExecute(item, e)"
    >
      <template #item="{ item, selected, hoverable, setRef, select }">
        <BaseListItem
          :ref="setRef"
          :id="`si-${item.id}`"
          :title="item.title"
          :hoverable="hoverable"
          :selected="selected"
          @click="select"
          @dblclick="() => handleExecute(item)"
        >
          <template #trailing>
            <ShortcutInput
              v-if="item.type === 'shortcut'"
              :model-value="String(item.value)"
              @update:model-value="item.update!"
            />
            <BaseSelect
              v-else-if="item.type === 'select'"
              :model-value="item.value"
              :options="item.options!"
              @update:model-value="item.update!"
            />
            <BaseButton
              v-else-if="item.type === 'button'"
              @click="item.action!"
            >
              清空
            </BaseButton>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { useSettingsInput, type SettingItem } from '@/composables/useSettingsInput'

const settings = useSettingsStore()
const appStore = useAppStore()
const { handleExecute } = useSettingsInput()

const clipboardShortcutValue = computed({
  get: () => settings.getShortcutOverride('clipboard') || 'CommandOrControl+Shift+C',
  set: (val: string) => settings.setShortcutOverride('clipboard', val),
})

const handleClipboardShortcutChange = async (val: string) => {
  await settings.setShortcutOverride('clipboard', val)
}

const maxDaysOptions = [
  { label: '15 天', value: 15 },
  { label: '30 天', value: 30 },
  { label: '90 天', value: 90 },
  { label: '永久', value: 0 },
]

const handleMaxDaysChange = async (val: number) => {
  if (!isNaN(val)) {
    await settings.setClipboardMaxDays(val)
  }
}

const handleClearHistory = async () => {
  const confirmed = await appStore.showConfirm({
    title: '清空剪贴板记录',
    message: '确定要清空所有未收藏的剪贴板记录吗？',
    kind: 'warning',
    okLabel: '确定',
    cancelLabel: '取消',
  })

  if (confirmed) {
    try {
      await invoke('clear_clipboard_history')
    } catch (e) {
      console.error('Failed to clear clipboard history:', e)
    }
  }
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'clipboard-shortcut',
    title: '启动快捷键',
    type: 'shortcut',
    value: clipboardShortcutValue.value,
    update: handleClipboardShortcutChange,
  },
  {
    id: 'clipboard-maxdays',
    title: '记录保留时长',
    type: 'select',
    options: maxDaysOptions,
    value: settings.clipboardMaxDays,
    update: handleMaxDaysChange,
  },
  {
    id: 'clipboard-clear',
    title: '清空未收藏记录',
    type: 'button',
    value: '',
    action: handleClearHistory,
  },
])

const selectedIndex = ref(0)
</script>
