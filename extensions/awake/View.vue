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
      <template #item="{ item, selected, hoverable, setRef, select }">
        <BaseListItem
          :ref="setRef"
          :title="item.title"
          :subtitle="item.subtitle"
          :hoverable="hoverable"
          :selected="selected"
          @click="select"
          @dblclick="handleExecute"
        >
          <template #trailing>
            <BaseButton
              @click="toggleAwake"
              :disabled="isLoading"
              :variant="isEnabled ? 'primary' : 'default'"
            >
              {{ isEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
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
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

const isEnabled = ref(false)
const isLoading = ref(true)
const selectedIndex = ref(0)
const appStore = useAppStore()

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke('is_awake_enabled')
  } catch (e) {
    console.error('Failed to check awake status:', e)
  } finally {
    isLoading.value = false
  }
}

const toggleAwake = async () => {
  isLoading.value = true
  try {
    const newState = !isEnabled.value
    await invoke('toggle_awake', { enable: newState })
    isEnabled.value = newState
  } catch (e) {
    console.error('Failed to toggle awake mode:', e)
  } finally {
    isLoading.value = false
  }
}

onMounted(() => {
  checkStatus()
})

const items = computed(() => [
  {
    id: 'awake',
    title: '保持系统唤醒',
    subtitle: '通过虚拟外接显示器实现，需要保持电源接入',
    groupId: 'power',
    groupTitle: '显示器',
  },
])

function handleExecute(_item?: unknown, _index?: number, e?: KeyboardEvent) {
  if (appStore.isComposing) return
  if (e) e.preventDefault()
  if (!isLoading.value) {
    toggleAwake()
  }
}
</script>
