<template>
  <div bg="surface" flex="~ col" h="screen" w="screen" shadow="lg">
    <div
      v-if="isDev"
      bg="accent"
      h="1"
      w="8"
      left="1/2"
      top="1"
      absolute
      class="-translate-x-1/2"
    />

    <!-- 搜索栏 -->
    <div
      ref="searchBarRef"
      p="x-5"
      border="b black/5"
      flex
      gap="3"
      h="15"
      items="center"
      @keydown="onSearchBarKeydown"
    >
      <!-- 扩展标签 -->
      <div
        v-if="activeModule"
        text="xs tx-secondary"
        p="x-3"
        rounded="md"
        bg="black/5"
        flex="~ none"
        gap="1.5"
        h="7"
        select="none"
        items="center"
        @mouseenter="isTagHovered = true"
        @mouseleave="isTagHovered = false"
      >
        <span shrink="0" h="3.5" w="3.5" relative>
          <Transition
            enter-active-class="transition duration-150 ease-out"
            enter-from-class="opacity-0 scale-75"
            enter-to-class="opacity-100 scale-100"
            leave-active-class="transition duration-100 ease-in"
            leave-from-class="opacity-100 scale-100"
            leave-to-class="opacity-0 scale-75"
          >
            <BaseButton
              v-if="isTagHovered"
              key="close"
              variant="ghost"
              class="rounded-full bg-black/10 flex-center inset-0 absolute !p-0 hover:bg-black/10 !h-3.5 !w-3.5"
              icon="i-ri-close-line text-xs text-tx-subtle"
              @click="onTagClose"
            />
            <span
              v-else
              key="icon"
              :class="activeModule.meta.icon"
              text="xs black/50"
              class="flex-center"
              h="3.5"
              w="3.5"
              inset="0"
              absolute
            ></span>
          </Transition>
        </span>
        <span>{{ activeModule.meta.name }}</span>
      </div>

      <input
        ref="searchInput"
        id="main-search-input"
        :value="appStore.searchQuery"
        :disabled="activeModule?.disableSearchInput"
        text="base black/85"
        outline="none"
        bg="transparent"
        flex="1"
        :class="'disabled:text-tx-primary placeholder:text-tx-hint disabled:opacity-100'"
        :placeholder="
          activeModule
            ? activeModule.disableSearchInput
              ? ''
              : activeModule.placeholder || `在 ${activeModule.meta.name} 中搜索`
            : '搜索应用或文件，输入 / 搜索扩展'
        "
        @input="onInput"
        @compositionstart="appStore.setComposing(true)"
        @compositionend="appStore.setComposing(false)"
      />

      <!-- 扩展附加区 -->
      <div
        v-if="activeModule?.searchBarAccessory"
        flex
        gap="2"
        min-w="0"
        items="center"
        overflow="hidden"
      >
        <component :is="activeModule.searchBarAccessory()" />
      </div>

      <!-- 更新提示按钮 -->
      <BaseButton
        v-if="updateStore.downloaded"
        icon="i-ri-arrow-up-circle-line text-accent"
        @click="updateStore.showDialog()"
      />
    </div>

    <!-- 内容区 -->
    <ContentView
      ref="contentViewRef"
      :module="activeModule"
      :results="results"
      :loading="isLoading"
      :selected-index="selectedIndex"
      :on-execute="activeModule ? undefined : handleExecute"
      :group-field="groupField"
      :group-title="groupTitle"
      @update:selected-index="(i: number) => (selectedIndex = i)"
    />

    <!-- 状态栏 -->
    <StatusBar
      :result-count="results.length"
      :selected-result="results[selectedIndex]"
      :is-loading="isLoading"
    />
  </div>

  <UpdateDialog v-if="updateStore.dialogVisible" @close="updateStore.closeDialog()" />
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useScroll } from '@/composables/events'
import { getExtension } from '@/runtime/extension-registry'
import { SEARCH } from '@/runtime/constants'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import type { SearchResult } from '@/runtime/types'
import ContentView from '@/components/layout/ContentView.vue'
import StatusBar from '@/components/layout/StatusBar.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import UpdateDialog from '@/components/ui/UpdateDialog.vue'

import { useScrollPosition } from '@/composables/useScrollPosition'
import { useSearchInput } from '@/composables/useSearchInput'
import { useResultNavigation } from '@/composables/useResultNavigation'
import { getFocusableElements, cycleFocus } from '@/utils/dom'

const isDev = import.meta.env.DEV
const appStore = useAppStore()
const updateStore = useUpdateStore()

const searchBarRef = ref<HTMLDivElement>()
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
  return (id ? getExtension(id) : null) ?? null
})

// 分组逻辑：按 kind 分组，file/folder 合并为"文件"；标题读 constants 单一源
const groupField = (item: SearchResult) => {
  const kind = item.data?.kind as string | undefined
  if (kind === 'file' || kind === 'folder') return 'file'
  return kind || 'other'
}

const groupTitle = (group: string) =>
  SEARCH.GROUP_TITLES[group as keyof typeof SEARCH.GROUP_TITLES] || group

const { onInput, handleTagClose, isLoading, clearSearch, loadDefaultResults, goBackToToolList } =
  useSearchInput({
    searchInput,
    results,
    selectedIndex,
    activeModule,
    restore,
    reset,
  })
const { handleExecute } = useResultNavigation({
  searchInput,
  results,
  selectedIndex,
  activeModule,
  clearSearch,
  loadDefaultResults,
  goBackToToolList,
})

// 进入模块子视图时释放搜索栏焦点，让键盘事件能到达子视图内容
// 注意：必须先把焦点转移到容器，否则窗口会因失焦而自动隐藏
// 滚动位置：进入 subview 时保存当前（module 列表）滚动 + subview 从顶开始；
//          返回时恢复 module 列表滚动（与 tools 列表 save/restore 同构）
watch(
  () => appStore.activeSubview,
  (val) => {
    if (val) {
      save('subview')
      reset()
      contentViewRef.value?.scrollContainer?.focus()
    } else {
      restore('subview')
      nextTick(() => searchInput.value?.focus())
    }
  },
)

// 进入模块时重置滚动位置，覆盖快捷键、open-module 事件等所有进入路径
// 从主列表进入时保存其滚动位置，goBackToToolList 调用 restore('tools') 恢复
watch(
  () => activeModule.value?.meta.id,
  (newId, oldId) => {
    if (newId && newId !== oldId) {
      if (!oldId) save('tools')
      reset()
    }
  },
)

function onSearchBarKeydown(e: KeyboardEvent) {
  if (e.key !== 'Tab') return
  const focusable = getFocusableElements(searchBarRef.value!)
  if (focusable.length === 0) return
  e.preventDefault()
  e.stopPropagation()
  cycleFocus(focusable, e)
}

function onTagClose() {
  isTagHovered.value = false
  handleTagClose()
}
</script>
