<template>
  <div class="flex flex-col h-full">
    <BaseList
      :items="settingsItems"
      v-model:selected-index="settingsSelectedIndex"
      @execute="onSettingsExecute"
    >
      <template #item="{ item, selected, setRef }">
        <BaseListItem
          v-if="item.type === 'toggle'"
          :ref="setRef"
          title="拖拽分屏"
          subtitle="拖动窗口到屏幕顶部触发"
          icon="i-ri-drag-move-2-line"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              :variant="settings.wmDragSnapEnabled ? 'primary' : 'default'"
              @click.stop="settings.setWmDragSnapEnabled(!settings.wmDragSnapEnabled)"
            >
              {{ settings.wmDragSnapEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
          </template>
        </BaseListItem>

        <BaseListItem
          v-else
          :ref="setRef"
          title="自定义尺寸"
          subtitle="自定义布局使用"
          icon="i-ri-ruler-line"
          :selected="selected"
        >
          <template #trailing>
            <div class="w-28" @click.stop>
              <BaseInput
                :model-value="customSizeDisplay"
                placeholder="800 × 600"
                @update:model-value="handleCustomSize"
                @keydown.enter.prevent
              />
            </div>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseInput from '@/components/ui/BaseInput.vue'

const settings = useSettingsStore()
const settingsSelectedIndex = ref(0)

interface ToggleItem {
  type: 'toggle'
  id: string
}

interface InputItem {
  type: 'input'
  id: string
}

type SettingsItem = ToggleItem | InputItem

const settingsItems: SettingsItem[] = [
  { type: 'toggle', id: 'wm-drag-snap' },
  { type: 'input', id: 'wm-custom-size' },
]

const customSizeDisplay = computed(() => `${settings.wmCustomWidth} × ${settings.wmCustomHeight}`)

function handleCustomSize(val: string) {
  const parts = val.split(/[×x*X\s]+/).map((s) => parseInt(s.trim(), 10))
  if (parts.length >= 2 && parts[0] > 0 && parts[1] > 0) {
    settings.setWmCustomWidth(parts[0])
    settings.setWmCustomHeight(parts[1])
  }
}

function onSettingsExecute(item: SettingsItem) {
  void item
}
</script>
