<template>
  <div class="flex-col-full">
    <BaseList :items="items" v-model:selected-index="selectedIndex" @execute="handleExecute">
      <template #item="{ item, selected, setRef }">
        <BaseListItem
          :ref="setRef"
          :title="item.title"
          :subtitle="item.subtitle"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton @click="enable" :variant="isEnabled ? 'primary' : 'default'">
              {{ isEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

const isEnabled = ref(false)
const selectedIndex = ref(0)
const appStore = useAppStore()
let unlistenFn: UnlistenFn | undefined

const enable = async () => {
  if (isEnabled.value) return
  try {
    await invoke(CMD.setCleanModeEnabled, { enabled: true })
    isEnabled.value = true
  } catch (e) {
    appStore.showStatus(`${e ?? '未知错误'}`, { duration: 6000, kind: 'error' })
  }
}

onMounted(async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isCleanModeEnabled)
  } catch (e) {
    console.error('[clean-mode] isCleanModeEnabled failed:', e)
  }
  unlistenFn = await listen('clean-mode-exit', () => {
    isEnabled.value = false
  })
})

onBeforeUnmount(() => {
  unlistenFn?.()
})

const items = computed(() => [
  {
    id: 'enable',
    title: '清洁模式',
    subtitle: '全屏黑屏、键鼠锁定，长按鼠标/触控板 2 秒退出',
  },
])

function handleExecute(item: unknown, _index?: number, e?: KeyboardEvent) {
  if (appStore.isComposing) return
  if (e) e.preventDefault()
  const i = item as { id: string } | undefined
  if (i?.id === 'enable') {
    enable()
  }
}
</script>
