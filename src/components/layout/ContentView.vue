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
        <KeepAlive v-if="resolvedView" :max="10">
          <component
            :is="resolvedView"
            :key="`${props.module?.id ?? 'main'}-${appStore.activeSubview ?? 'view'}`"
          />
        </KeepAlive>

        <!-- Standard list -->
        <template v-if="!resolvedView">
          <BaseEmptyState v-if="currentLoading && currentResults.length === 0" :loading="true" />

          <BaseEmptyState
            v-else-if="currentResults.length === 0"
            :title="
              module
                ? module.placeholder || `在 ${module.name} 中无结果`
                : '搜索应用或文件，输入 / 搜索扩展'
            "
            :icon="module ? module.icon : 'i-ri-search-line'"
          />

          <BaseList
            v-else
            :items="currentResults"
            :selected-index="currentSelectedIndex"
            :multi-select="isMultiSelect"
            :selected-ids="selectedIds"
            :keyboard-active="!!appStore.activeModuleId"
            :composing="appStore.isComposing"
            @update:selected-ids="selectedIds = $event"
            :group-field="!module ? groupField : undefined"
            :group-title="!module ? groupTitle : undefined"
            @update:selected-index="handleUpdateSelectedIndex"
            @execute="handleExecute"
            @reveal="handleReveal"
          >
            <template #item="{ item, selected, multiSelected, setRef }">
              <BaseListItem
                :ref="setRef"
                :selected="selected || multiSelected"
                :icon-wrapper-class="item.data?.icon && !item.icon?.startsWith('i-') && item.data?.kind !== 'module' ? 'bg-transparent' : undefined"
              >
                <template #icon>
                  <ResultIcon :item="item" :module-icon="module?.icon" />
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
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, provide } from 'vue'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { hideWindow } from '@/utils/tauri'
import { scoreFields } from '@/utils/fuzzy'
import type { AppModule, ModuleSearchItem, SearchResult } from '@/types/module'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import ResultIcon from '@/components/layout/ResultIcon.vue'
import { getParentPath, formatPathParts } from '@/utils/format'

const props = defineProps<{
  module?: AppModule | null
  results?: SearchResult[]
  initialLoading?: boolean
  selectedIndex?: number
  groupField?: (item: SearchResult) => string
  groupTitle?: (group: string) => string
  onExecute?: (result: SearchResult) => void
}>()

const emit = defineEmits<{
  'update:selectedIndex': [value: number]
}>()

const appStore = useAppStore()

const internalResults = ref<SearchResult[]>([])
const internalLoading = ref(false)
const internalSelectedIndex = ref(0)
const selectedIds = ref(new Set<string>())
let debounceTimer: ReturnType<typeof setTimeout>

const isMultiSelect = computed(() => !!props.module?.listOptions?.multiSelect)

const currentResults = computed(() => props.results ?? internalResults.value)
const currentLoading = computed(() => {
  if (props.initialLoading) return true
  return props.results !== undefined ? false : internalLoading.value
})
const currentSelectedIndex = computed(() => props.selectedIndex ?? internalSelectedIndex.value)

/**
 * 将布局决策逻辑收拢到一处。
 * subview 模式：使用模块声明的命名子视图，占满整个内容区。
 * view 模式：使用模块声明的 view。
 */
const resolvedView = computed(() => {
  const subviewId = appStore.activeSubview
  if (subviewId && props.module?.subviews?.[subviewId]) {
    return props.module.subviews[subviewId]
  }
  return props.module?.view
})

const scrollContainer = ref<HTMLElement>()
defineExpose({ scrollContainer })

function itemToSearchResult(item: ModuleSearchItem): SearchResult {
  return {
    id: item.id,
    title: item.title,
    description: item.subtitle,
    icon: item.icon,
    module: props.module?.id ?? '',
    score: 100,
    data: { kind: 'module', ...item },
  }
}

const filteredItems = computed(() => internalResults.value.map((r) => r.data as ModuleSearchItem))
provide('filteredItems', filteredItems)

const doSearch = async (query: string) => {
  if (props.results !== undefined) return

  if (props.module?.searchItems) {
    const items = props.module.searchItems()
    if (!query.trim()) {
      internalResults.value = items.map(itemToSearchResult)
    } else {
      internalResults.value = items
        .map((item) => ({
          item,
          score: scoreFields([item.title, item.subtitle ?? '', ...item.keywords], query),
        }))
        .filter((entry) => entry.score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ item }) => itemToSearchResult(item))
    }
    internalLoading.value = false
    return
  }

  if (!props.module?.onModuleSearch) return

  internalLoading.value = true
  try {
    const res = await props.module.onModuleSearch(query)
    internalResults.value = res
    if (internalSelectedIndex.value >= res.length) {
      internalSelectedIndex.value = 0
    }
  } catch (e) {
    console.error(`[ContentView] ${props.module.id} search error:`, e)
    internalResults.value = []
  } finally {
    internalLoading.value = false
  }
}

watch(
  () => appStore.searchQuery,
  (newQuery) => {
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => doSearch(newQuery), 100)
  },
)

watch(
  () => props.module?.id,
  (newId, oldId) => {
    if (newId && newId !== oldId) {
      clearTimeout(debounceTimer)
      internalResults.value = []
      internalSelectedIndex.value = 0
      selectedIds.value = new Set()
      doSearch(appStore.searchQuery)
    }
  },
)

onMounted(() => {
  doSearch(appStore.searchQuery)
})
onUnmounted(() => {
  clearTimeout(debounceTimer)
})

const handleExecute = async (result: SearchResult) => {
  const multiResults =
    isMultiSelect.value && selectedIds.value.size > 0
      ? currentResults.value.filter((r) => selectedIds.value.has(r.id))
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
    await invoke('reveal_in_finder', { path: result.data.path })
    hideWindow()
  }
}

const handleUpdateSelectedIndex = (i: number) => {
  if (props.results !== undefined) {
    emit('update:selectedIndex', i)
  } else {
    internalSelectedIndex.value = i
  }
}
const isFileOrFolder = (item: SearchResult) =>
  item.data?.kind === 'file' || item.data?.kind === 'folder'
</script>
