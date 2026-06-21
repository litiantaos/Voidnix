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
          title="启用扩展功能"
          :subtitle="authorized === false ? '开启后将引导你到系统设置中启用扩展' : undefined"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              :variant="finderConfig.enabled ? 'primary' : 'default'"
              @click.stop="toggle"
            >
              {{ finderConfig.enabled ? '已开启' : '已关闭' }}
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
import { CMD } from '@/commands'
import { config as finderConfig } from './config'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

const appStore = useAppStore()

const authorized = ref<boolean | null>(null)

async function checkAuthorized() {
  try {
    authorized.value = await invoke<boolean>(CMD.checkFinderExtAuthorized)
  } catch {
    authorized.value = false
  }
}

checkAuthorized()

const toggle = async () => {
  const newVal = !finderConfig.enabled
  // H11：显式 invoke 同步 Rust 状态；成功才回写 config，失败给用户反馈
  try {
    await invoke(CMD.setFinderExtEnabled, { enabled: newVal })
  } catch (e) {
    appStore.showStatus(`开关失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
    return
  }
  finderConfig.enabled = newVal
  if (newVal && !authorized.value) {
    try {
      await invoke(CMD.openExtensionsPrefs)
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
