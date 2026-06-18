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
              :model-value="awakeConfig.mirrorMode ? 'mirror' : 'extend'"
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
    isEnabled.value = await invoke('is_awake_enabled')
    await invoke('set_awake_mode', { mirror: awakeConfig.mirrorMode })
  } catch (e) {
    console.error('Failed to check awake status:', e)
  }
}

const toggleAwake = async () => {
  try {
    const newState = !isEnabled.value
    await invoke('toggle_awake', { enable: newState })
    isEnabled.value = newState
  } catch (e) {
    console.error('Failed to toggle awake mode:', e)
  }
}

const modeOptions = [
  { label: '镜像', value: 'mirror' },
  { label: '扩展', value: 'extend' },
]

const onModeChange = async (value: string | number) => {
  const mirror = value === 'mirror'
  try {
    await invoke('set_awake_mode', { mirror })
    awakeConfig.mirrorMode = mirror
  } catch (e) {
    console.error('Failed to set awake mode:', e)
  }
}

onMounted(() => {
  checkStatus()
})

const items = computed(() => [
  {
    id: 'awake',
    title: '保持系统唤醒',
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
