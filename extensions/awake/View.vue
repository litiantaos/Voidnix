<template>
  <div class="flex flex-col h-full">
    <BaseList
      :items="items"
      v-model:selected-index="selectedIndex"
      keyboard-navigation
      group-field="groupId"
      @execute="handleExecute"
    >
      <template #group-title="{ item }">
        {{ item.groupTitle }}
      </template>
      <template #item="{ item, selected, setRef, select }">
        <BaseListItem
          :ref="setRef"
          :title="item.title"
          :subtitle="item.subtitle"
          :selected="selected"
          @click="select"
          @dblclick="() => handleExecute(item)"
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
              :model-value="isMirrorMode ? 'mirror' : 'extend'"
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
import { useSettingsStore } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const isEnabled = ref(false)
const isMirrorMode = ref(true)
const selectedIndex = ref(0)
const appStore = useAppStore()
const settings = useSettingsStore()

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke('is_awake_enabled')
    isMirrorMode.value = settings.awakeMirrorMode
    await invoke('set_awake_mode', { mirror: isMirrorMode.value })
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
  isMirrorMode.value = mirror
  try {
    await invoke('set_awake_mode', { mirror })
    await settings.setAwakeMirrorMode(mirror)
  } catch (e) {
    isMirrorMode.value = !mirror
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
