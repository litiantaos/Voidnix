<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useScroll } from '@vueuse/core'
import { getModule } from '@/core/module-registry'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import type { SearchResult } from '@/types/module'
import ContentView from '@/components/layout/ContentView.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import UpdateDialog from '@/components/ui/UpdateDialog.vue'

import { useScrollPosition } from '@/composables/useScrollPosition'
import { useSearchCommand } from '@/composables/useSearchCommand'

const appStore = useAppStore()
const updateStore = useUpdateStore()
const showUpdateDialog = ref(false)

const searchInput = ref<HTMLInputElement>()
const contentViewRef = ref<InstanceType<typeof ContentView>>()
const results = ref<SearchResult[]>([])
const selectedIndex = ref(0)
const isTagHovered = ref(false)

const scrollContainer = computed(() => contentViewRef.value?.scrollContainer)
const { y: scrollTop } = useScroll(scrollContainer)
const { save, restore, reset } = useScrollPosition(scrollTop)

const activeModule = computed(() => {
  const id = appStore.activeModuleId
  return (id ? getModule(id) : null) ?? null
})

const kindMap: Record<string, string> = {
  application: '应用',
  file_and_folder: '文件',
  module: '扩展',
  clipboard: '剪贴板',
}

function getGroupKey(result: SearchResult): string {
  const k = (result.data?.kind as string) || 'other'
  return k === 'file' || k === 'folder' ? 'file_and_folder' : k
}

function getGroupTitle(group: string): string {
  return kindMap[group] || ''
}

const { onInput, handleExecute, handleTagClose, isLoading } = useSearchCommand({
  searchInput,
  results,
  selectedIndex,
  activeModule,
  save,
  restore,
  reset,
})

// 进入设置面板时释放搜索栏焦点，让键盘事件能到达设置列表
// 注意：必须先把焦点转移到容器，否则窗口会因失焦而自动隐藏
watch(
  () => appStore.showSettings,
  (val) => {
    if (val) {
      contentViewRef.value?.scrollContainer?.focus()
    } else {
      nextTick(() => searchInput.value?.focus())
    }
  },
)

function onTagClose() {
  isTagHovered.value = false
  handleTagClose()
}
</script>

<template>
  <div class="bg-surface flex flex-col h-screen w-screen shadow-lg">
    <!-- 搜索栏 -->
    <div class="px-5 border-b border-black/5 flex gap-3 h-15 items-center">
      <div
        v-if="activeModule"
        class="text-xs text-black/70 px-3 rounded-md bg-black/5 flex flex-none gap-1.5 h-7 cursor-default select-none items-center relative"
        @mouseenter="isTagHovered = true"
        @mouseleave="isTagHovered = false"
      >
        <span class="shrink-0 h-3.5 w-3.5 relative">
          <Transition
            enter-active-class="transition duration-150 ease-out"
            enter-from-class="opacity-0 scale-75"
            enter-to-class="opacity-100 scale-100"
            leave-active-class="transition duration-100 ease-in"
            leave-from-class="opacity-100 scale-100"
            leave-to-class="opacity-0 scale-75"
          >
            <button
              v-if="isTagHovered"
              key="close"
              class="rounded-full bg-black/10 flex h-3.5 w-3.5 transition-colors duration-150 items-center inset-0 justify-center absolute hover:bg-black/20"
              @click="onTagClose"
            >
              <span class="i-ri-close-line text-[10px] text-black/60"></span>
            </button>
            <span
              v-else
              key="icon"
              :class="activeModule.icon"
              class="text-[10px] text-black/50 flex h-3.5 w-3.5 items-center inset-0 justify-center absolute"
            ></span>
          </Transition>
        </span>
        <span>{{ activeModule.name }}</span>
      </div>
      <input
        ref="searchInput"
        id="main-search-input"
        :value="appStore.searchQuery"
        :readonly="activeModule?.multiline"
        class="text-base text-black/85 outline-none bg-transparent flex-1 placeholder:text-black/25"
        :class="{ 'cursor-default': activeModule?.multiline }"
        :placeholder="
          activeModule
            ? activeModule.multiline
              ? ''
              : activeModule.placeholder || `在 ${activeModule.name} 中搜索`
            : '搜索应用或文件，输入 / 搜索扩展'
        "
        @input="onInput"
        @compositionstart="appStore.setComposing(true)"
        @compositionend="appStore.setComposing(false)"
      />
      <div
        v-if="activeModule?.toolbar"
        class="flex gap-2 min-w-0 items-center overflow-hidden"
      >
        <span
          v-if="typeof activeModule.toolbar === 'string'"
          class="text-xs text-black/30"
        >
          {{ activeModule.toolbar }}
        </span>
        <component :is="activeModule.toolbar" v-else />
      </div>

      <!-- 更新提示按钮 -->
      <BaseButton
        v-if="updateStore.downloaded"
        size="icon"
        @click="showUpdateDialog = true"
      >
        <div class="i-ri-arrow-up-circle-line text-accent"></div>
      </BaseButton>
    </div>

    <!-- 内容区 -->
    <ContentView
      ref="contentViewRef"
      :module="activeModule"
      :results="activeModule ? undefined : results"
      :initial-loading="!activeModule && isLoading && results.length === 0"
      :selected-index="activeModule ? undefined : selectedIndex"
      :group-field="activeModule ? undefined : getGroupKey"
      :group-title="activeModule ? undefined : getGroupTitle"
      :on-execute="activeModule ? undefined : handleExecute"
      @update:selected-index="(i: number) => (selectedIndex = i)"
    />
  </div>

  <UpdateDialog v-if="showUpdateDialog" @close="showUpdateDialog = false" />
</template>
