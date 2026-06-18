<template>
  <div class="flex-col-full-pb">
    <BaseList
      :items="items"
      v-model:selected-index="selectedIndex"
      :group-field="(item: Item) => item.group"
      :group-title="(g: string) => g"
      @execute="(item: Item) => onExecute(item)"
    >
      <template #item="{ item, selected, setRef }">
        <!-- 快捷键 -->
        <BaseListItem
          v-if="item.type === 'shortcut'"
          :ref="setRef"
          :id="`si-${SHORTCUT_ITEM_ID}`"
          title="启动快捷键"
          :selected="selected"
        >
          <template #trailing>
            <ShortcutInput
              :model-value="screenshotShortcutValue"
              shortcut-id="screenshot"
              @update:model-value="handleShortcutChange"
            />
          </template>
        </BaseListItem>

        <!-- 保存路径 -->
        <BaseListItem
          v-else-if="item.type === 'savePath'"
          :ref="setRef"
          title="截图保存位置"
          :subtitle="savePathDisplay(screenshotConfig.savePath)"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton icon="i-ri-folder-open-line" @click.stop="pickSavePath" />
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { config as screenshotConfig } from './config'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { useSettingsInput } from '@/composables/useSettingsInput'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const appStore = useAppStore()
useSettingsInput()

const SHORTCUT_ITEM_ID = 'screenshot-shortcut'

const { value: screenshotShortcutValue, update: handleShortcutChange } = useShortcutConfig(
  'screenshot',
  'CommandOrControl+Shift+X',
)

async function pickSavePath() {
  // NSOpenPanel 运行期间抑制失焦隐藏
  appStore.suppressBlur = true
  try {
    const selected = await invoke<string>('pick_directory')
    if (selected) {
      await (screenshotConfig.savePath = selected)
    }
  } finally {
    setTimeout(() => {
      appStore.suppressBlur = false
    }, 800)
  }
}

function savePathDisplay(path: string): string {
  if (!path) return '~/Downloads'
  return path.replace(/^\/Users\/[^/]+/, '~')
}

// ── 列表项 ─────────────────────────────────────────────────
interface ShortcutItem {
  type: 'shortcut'
  group: string
}
interface SavePathItem {
  type: 'savePath'
  group: string
}
type Item = ShortcutItem | SavePathItem

const items: Item[] = [
  { type: 'shortcut', group: '通用' },
  { type: 'savePath', group: '通用' },
]

const selectedIndex = ref(0)

function onExecute(item: Item) {
  if (item.type === 'savePath') {
    pickSavePath()
  }
}
</script>
