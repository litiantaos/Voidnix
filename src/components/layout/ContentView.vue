<template>
  <div flex="~ 1 col" overflow="hidden">
    <!-- Scrollable Content -->
    <div
      ref="scrollContainer"
      tabindex="-1"
      outline="none"
      flex="~ 1 col"
      min-h="0"
      relative
      class="hide-scrollbar overflow-y-auto"
    >
      <div flex="~ 1 col">
        <!-- max 覆盖全部视图 key（9 模块 mainView + screenshot{ocr} 子视图 + settings 钻入的扩展 settingsView），
             驱逐会导致模块 View 重挂载（重渲染列表/消息）→ 切换卡顿，故留充裕余量 -->
        <KeepAlive v-if="resolvedView" :max="24">
          <component
            :is="resolvedView"
            :key="`${props.module?.meta.id ?? 'main'}-${appStore.activeSubview ?? 'view'}`"
          />
        </KeepAlive>

        <!-- Standard list -->
        <template v-if="!resolvedView">
          <Transition
            mode="out-in"
            enter-active-class="transition duration-100 ease-out"
            enter-from-class="opacity-0"
            leave-active-class="transition duration-100 ease-in"
            leave-to-class="opacity-0"
          >
            <BaseEmptyState
              v-if="props.loading && props.results.length === 0"
              :key="'loading'"
              :loading="true"
            />

            <BaseEmptyState
              v-else-if="props.results.length === 0"
              :key="'empty'"
              :title="
                module
                  ? module.placeholder || `在 ${module.meta.name} 中无结果`
                  : '搜索应用或文件，输入 / 搜索扩展'
              "
              :icon="module ? module.meta.icon : 'i-ri-search-line'"
            />

            <BaseList
              v-else
              :key="'list'"
              :items="props.results"
              :selected-index="props.selectedIndex"
              :multi-select="isMultiSelect"
              :selected-ids="selectedIds"
              :keyboard-active="!!appStore.activeModuleId"
              :composing="appStore.isComposing"
              @update:selected-ids="selectedIds = $event"
              :group-field="!module ? props.groupField : undefined"
              :group-title="!module ? props.groupTitle : undefined"
              @update:selected-index="(i: number) => emit('update:selectedIndex', i)"
              @execute="handleExecute"
              @reveal="handleReveal"
            >
              <template #item="{ item, selected, multiSelected, setRef }">
                <BaseListItem
                  :ref="setRef"
                  :selected="selected || multiSelected"
                  :icon-wrapper-class="
                    item.data?.icon && !item.icon?.startsWith('i-') && item.data?.kind !== 'module'
                      ? 'bg-transparent'
                      : undefined
                  "
                >
                  <template #icon>
                    <ResultIcon :item="item" :module-icon="module?.meta.icon" />
                  </template>
                  <template #title>
                    <div :class="item.data?.isHighlight ? 'text-accent font-medium' : ''">
                      {{ item.title }}
                    </div>
                  </template>
                  <template #subtitle>
                    <span v-if="item.description" truncate>{{ item.description }}</span>
                    <template v-else-if="item.data?.path && isFileOrFolder(item)">
                      <span
                        class="flex-[0_1_auto] min-w-0 truncate"
                        :title="getParentPath(item.data.path)"
                      >
                        {{ formatPathParts(getParentPath(item.data.path)).head }}
                      </span>
                      <span flex="none" whitespace="nowrap">
                        {{ formatPathParts(getParentPath(item.data.path)).tail }}
                      </span>
                    </template>
                  </template>
                </BaseListItem>
              </template>
            </BaseList>
          </Transition>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { hideWindow } from '@/utils/tauri'
import { getExtension } from '@/runtime/extension-registry'
import type { Extension, SearchResult } from '@/runtime/types'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import ResultIcon from '@/components/layout/ResultIcon.vue'
import { getParentPath, formatPathParts } from '@/utils/format'

const props = defineProps<{
  module?: Extension | null
  results: SearchResult[]
  loading?: boolean
  selectedIndex: number
  groupField?: (item: SearchResult) => string
  groupTitle?: (group: string) => string
  onExecute?: (result: SearchResult) => void
}>()

const emit = defineEmits<{
  'update:selectedIndex': [value: number]
}>()

const appStore = useAppStore()

const selectedIds = ref(new Set<string>())
const isMultiSelect = computed(() => !!props.module?.listOptions?.multiSelect)

/**
 * 纯渲染器：布局决策收拢至此，搜索编排由 useSearchInput 统一承担（结果经 props 注入）。
 * subview 模式优先级：
 *   1. 当前模块的私有命名子视图（screenshot{ocr}）
 *   2. 跨扩展 settingsView 导航（settings 枢纽钻入目标扩展的 settingsView，§2.2 N3）
 * mainView 模式：使用扩展声明的主视图。
 */
const resolvedView = computed(() => {
  const subviewId = appStore.activeSubview
  if (subviewId) {
    if (props.module?.subviews?.[subviewId]) {
      return props.module.subviews[subviewId]()
    }
    const target = getExtension(subviewId)
    if (target?.settingsView) {
      return target.settingsView()
    }
  }
  return props.module?.mainView?.()
})

const scrollContainer = ref<HTMLElement>()
defineExpose({ scrollContainer })

const handleExecute = async (result: SearchResult) => {
  const multiResults =
    isMultiSelect.value && selectedIds.value.size > 0
      ? props.results.filter((r) => selectedIds.value.has(r.id))
      : undefined
  if (multiResults) selectedIds.value = new Set()
  if (props.onExecute) {
    props.onExecute(result)
  } else if (props.module?.onExecute) {
    await props.module.onExecute(result, multiResults)
  }
}

const handleReveal = async (result: SearchResult) => {
  if (result.data?.path) {
    await invoke(CMD.revealInFinder, { path: result.data.path })
    hideWindow()
  }
}

const isFileOrFolder = (item: SearchResult) =>
  item.data?.kind === 'file' || item.data?.kind === 'folder'
</script>
