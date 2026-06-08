<template>
  <div class="flex-col-full-pb">
    <BaseList
      :items="allItems"
      v-model:selected-index="selectedIndex"
      :group-field="(item: FinderExtItem) => item.group"
      :group-title="(g: string) => g"
      @execute="(item: FinderExtItem) => item.type === 'toggle' && toggle()"
    >
      <template #item="{ item, selected, setRef }">
        <BaseListItem
          v-if="item.type === 'toggle'"
          :ref="setRef"
          title="访达右键菜单"
          subtitle="开启后将引导你到系统设置中启用扩展"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              :variant="settings.finderExtEnabled ? 'primary' : 'default'"
              @click.stop="toggle"
            >
              {{ settings.finderExtEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
          </template>
        </BaseListItem>

        <BaseListItem
          v-else
          :ref="setRef"
          :title="(item as ActionItem).title"
          :selected="selected"
        />
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

const settings = useSettingsStore()

const toggle = async () => {
  const newVal = !settings.finderExtEnabled
  await settings.setFinderExtEnabled(newVal)
  if (newVal) {
    try {
      await invoke('open_extensions_prefs')
    } catch (e) {
      console.error('Failed to open extensions prefs:', e)
    }
  }
}

interface ActionItem {
  type: 'action'
  group: string
  id: string
  title: string
}

interface ToggleItem {
  type: 'toggle'
  group: string
}

type FinderExtItem = ToggleItem | ActionItem

const allItems: FinderExtItem[] = [
  { type: 'toggle', group: '通用' },
  { type: 'action', group: '菜单项', id: 'copy-path', title: '拷贝路径' },
  {
    type: 'action',
    group: '菜单项',
    id: 'open-terminal',
    title: '在终端中打开',
  },
  { type: 'action', group: '菜单项', id: 'new-file', title: '新建文件' },
  {
    type: 'action',
    group: '菜单项',
    id: 'toggle-hidden',
    title: '切换隐藏文件',
  },
]

const selectedIndex = ref(0)
</script>
