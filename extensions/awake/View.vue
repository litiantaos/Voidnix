<template>
  <div class="flex-col-full">
    <BaseList
      :items="items"
      v-model:selected-index="selectedIndex"
      group-field="groupId"
      @execute="handleExecute"
    >
      <template #group-title="{ item }">
        {{ item.groupTitle }}
      </template>
      <template #item="{ item, selected, setRef }">
        <BaseListItem
          :ref="setRef"
          :title="item.title"
          :subtitle="item.subtitle"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              v-if="item.id === 'awake'"
              @click="toggleAwake"
              :variant="isEnabled ? 'primary' : 'default'"
            >
              {{ isEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
            <BaseSelect
              v-else-if="item.id === 'mode'"
              :model-value="awakeConfig.displayMode"
              :options="modeOptions"
              @update:model-value="onModeChange"
            />
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { config as awakeConfig } from './config'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const isEnabled = ref(false)
const selectedIndex = ref(0)
const appStore = useAppStore()

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isAwakeEnabled)
  } catch (e) {
    console.error('Failed to check awake status:', e)
  }
}

const toggleAwake = async () => {
  const newState = !isEnabled.value
  try {
    await invoke(CMD.setAwakeEnabled, { enabled: newState })
    isEnabled.value = newState
  } catch (e) {
    appStore.showStatus(`切换失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
  }
}

const modeOptions = [
  { label: '镜像', value: 'mirror' },
  { label: '扩展', value: 'extend' },
]

const onModeChange = (value: string | number) => {
  awakeConfig.displayMode = value as typeof awakeConfig.displayMode
}

onMounted(() => {
  checkStatus()
})

const items = computed(() => [
  {
    id: 'awake',
    title: '启用扩展功能',
    subtitle: '通过虚拟外接显示器触发 Clamshell Mode，需接入电源',
    groupId: 'power',
    groupTitle: '显示器',
  },
  {
    id: 'mode',
    title: '显示模式',
    subtitle: '镜像与主屏显示相同画面，扩展提供独立桌面空间',
    groupId: 'power',
    groupTitle: '显示器',
  },
])

function handleExecute(item: unknown, _index?: number, e?: KeyboardEvent) {
  if (appStore.isComposing) return
  if (e) e.preventDefault()
  const i = item as { id: string } | undefined
  if (i?.id === 'awake') {
    toggleAwake()
  }
}
</script>
