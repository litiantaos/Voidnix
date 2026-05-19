<template>
  <div class="pb-4 flex flex-col h-full">
    <BaseList
      :items="items"
      v-model:selected-index="selectedIndex"
      keyboard-navigation
      :group-field="(item: Item) => item.group"
      :group-title="(g: string) => g"
      @execute="(item: Item) => onExecute(item)"
    >
      <template #item="{ item, selected, hoverable: h, setRef, select }">
        <!-- 快捷键 -->
        <BaseListItem
          v-if="item.type === 'shortcut'"
          :ref="setRef"
          :id="`si-${SHORTCUT_ITEM_ID}`"
          title="启动快捷键"
          :selected="selected"
          :hoverable="h"
          @click="select"
        >
          <template #trailing>
            <ShortcutInput
              :ref="(el) => setShortcutRef(`si-${SHORTCUT_ITEM_ID}`, el)"
              :model-value="screenshotShortcutValue"
              @update:model-value="handleShortcutChange"
            />
          </template>
        </BaseListItem>

        <!-- 保存路径 -->
        <BaseListItem
          v-else-if="item.type === 'savePath'"
          :ref="setRef"
          title="截图保存位置"
          :subtitle="savePathDisplay(settings.screenshotSavePath)"
          :selected="selected"
          :hoverable="h"
          @click="select"
          @dblclick="pickSavePath"
        >
          <template #trailing>
            <BaseButton @click.stop="pickSavePath">
              <div class="i-ri-folder-open-line text-sm"></div>
            </BaseButton>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { useSettingsInput } from '@/composables/useSettingsInput'

const settings = useSettingsStore()
const appStore = useAppStore()
const { setShortcutRef, shortcutRefs } = useSettingsInput()

const SHORTCUT_ITEM_ID = 'screenshot-shortcut'

const screenshotShortcutValue = computed(
  () => settings.getShortcutOverride('screenshot') || 'CommandOrControl+Shift+X',
)

const handleShortcutChange = async (val: string) => {
  await settings.setShortcutOverride('screenshot', val)
}

async function pickSavePath() {
  // NSOpenPanel 运行期间抑制失焦隐藏
  appStore.suppressBlur = true
  try {
    const selected = await invoke<string>('pick_directory')
    if (selected) {
      await settings.setScreenshotSavePath(selected)
    }
  } finally {
    setTimeout(() => { appStore.suppressBlur = false }, 800)
  }
}

function savePathDisplay(path: string): string {
  if (!path) return '~/Downloads'
  return path.replace(/^\/Users\/[^/]+/, '~')
}

// ── 列表项 ─────────────────────────────────────────────────
interface ShortcutItem { type: 'shortcut'; group: string }
interface SavePathItem { type: 'savePath'; group: string }
type Item = ShortcutItem | SavePathItem

const items: Item[] = [
  { type: 'shortcut', group: '通用' },
  { type: 'savePath', group: '通用' },
]

const selectedIndex = ref(0)

function onExecute(item: Item) {
  if (item.type === 'shortcut') {
    const r = shortcutRefs.value[`si-${SHORTCUT_ITEM_ID}`]
    if (r) { r.focus(); r.startRecording() }
  } else {
    pickSavePath()
  }
}
</script>
