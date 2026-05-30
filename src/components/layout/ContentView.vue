<template>
  <div class="flex flex-1 flex-col overflow-hidden">
    <!-- Header -->
    <div v-if="resolvedLayout.header" class="text-xs px-5 py-2 border-b border-black/5 shrink-0">
      <component :is="resolvedLayout.header" />
    </div>

    <!-- Scrollable Content -->
    <div
      ref="scrollContainer"
      tabindex="-1"
      class="hide-scrollbar outline-none flex flex-1 flex-col min-h-0 relative overflow-y-auto"
    >
      <div class="flex flex-1 flex-col">
        <KeepAlive v-if="resolvedLayout.view" :max="10">
          <component
            :is="resolvedLayout.view"
            :key="`${props.module?.id ?? 'main'}-${appStore.showPanel ? 'panel' : 'view'}`"
          />
        </KeepAlive>

        <!-- Standard list -->
        <template v-if="!resolvedLayout.view">
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
            keyboard-navigation
            :group-field="!module ? groupField : undefined"
            :group-title="!module ? groupTitle : undefined"
            @update:selected-index="handleUpdateSelectedIndex"
            @execute="handleExecute"
          >
            <template #item="{ item, selected, setRef, select }">
              <BaseListItem
                :ref="setRef"
                :selected="selected"
                :icon-wrapper-class="getIconWrapperClass(item)"
                @click="select"
                @dblclick="handleExecute(item)"
              >
                <template #icon>
                  <div
                    v-if="isIconFont(item) && !isModuleItem(item)"
                    class="flex h-6 w-6 items-center justify-center"
                  >
                    <i :class="[getIcon(item), 'text-xl text-black/50']" />
                  </div>
                  <img
                    v-else-if="isImageIcon(item) && !isModuleItem(item)"
                    :src="getIconSrc(item)"
                    class="h-[115%] max-w-[115%] w-[115%] object-contain"
                    :class="{ rounded: item.data?.iconStyle === 'rounded' }"
                    :alt="item.title"
                  />
                  <div
                    v-else-if="isModuleItem(item)"
                    class="text-sm text-accent rounded-md bg-accent/10 flex h-full w-full items-center justify-center"
                  >
                    <i :class="getIcon(item) || 'i-ri-apps-2-line'" />
                  </div>
                  <div
                    v-else-if="isFileOrFolder(item)"
                    class="rounded-md bg-black/4 flex h-full w-full items-center justify-center"
                  >
                    <i :class="[getFileIcon(item).icon, getFileIcon(item).color]" class="text-sm" />
                  </div>
                  <span v-else class="text-sm text-black/30 font-medium">
                    {{ item.title[0]?.toUpperCase() }}
                  </span>
                </template>
                <template #title>
                  <div :class="item.data?.isHighlight ? 'text-accent font-medium' : ''">
                    {{ item.title }}
                  </div>
                </template>
                <template #subtitle>
                  <span v-if="item.description" class="truncate">{{ item.description }}</span>
                  <template v-else-if="item.data?.path && isFileOrFolder(item)">
                    <span
                      class="flex-[0_1_auto] min-w-0 truncate"
                      :title="getParentPath(item.data.path)"
                    >
                      {{ formatPathParts(getParentPath(item.data.path)).head }}
                    </span>
                    <span class="flex-none whitespace-nowrap">
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

    <!-- Footer -->
    <div v-if="resolvedLayout.footer" class="shrink-0">
      <component :is="resolvedLayout.footer" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import type { AppModule, SearchResult } from '@/types/module'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import { getParentPath, formatPathParts } from '@/utils/format'
import { getFileIcon } from '@/utils/icons'

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
let debounceTimer: ReturnType<typeof setTimeout>

const currentResults = computed(() => props.results ?? internalResults.value)
const currentLoading = computed(() => {
  if (props.initialLoading) return true
  return props.results !== undefined ? false : internalLoading.value
})
const currentSelectedIndex = computed(() => props.selectedIndex ?? internalSelectedIndex.value)

/**
 * 将布局决策逻辑收拢到一处。
 * panel 模式：独立视图，无 chrome，占满整个内容区。
 * view 模式：使用模块声明的 layout（含 header/footer chrome）。
 */
const resolvedLayout = computed(() => {
  if (appStore.showPanel && props.module?.panel) {
    return { header: undefined, view: props.module.panel, footer: undefined }
  }
  return {
    header: props.module?.layout?.header,
    view: props.module?.layout?.view,
    footer: props.module?.layout?.footer,
  }
})

const scrollContainer = ref<HTMLElement>()
defineExpose({ scrollContainer })

const doSearch = async (query: string) => {
  if (props.results !== undefined) return
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
      doSearch(appStore.searchQuery)
    }
  },
)

onMounted(() => doSearch(appStore.searchQuery))
onUnmounted(() => clearTimeout(debounceTimer))

const handleExecute = async (result: SearchResult) => {
  if (props.onExecute) {
    props.onExecute(result)
  } else if (props.module?.onExecute) {
    await props.module.onExecute(result)
  }
}

const handleUpdateSelectedIndex = (i: number) => {
  if (props.results !== undefined) {
    emit('update:selectedIndex', i)
  } else {
    internalSelectedIndex.value = i
  }
}

const getIcon = (item: SearchResult) => item.icon || item.data?.icon || props.module?.icon
const isIconFont = (item: SearchResult) => getIcon(item)?.startsWith('i-')
const isImageIcon = (item: SearchResult) => getIcon(item) && !isIconFont(item)
const isModuleItem = (item: SearchResult) => item.data?.kind === 'module'
const isFileOrFolder = (item: SearchResult) =>
  item.data?.kind === 'file' || item.data?.kind === 'folder'
const getIconWrapperClass = (item: SearchResult) =>
  isImageIcon(item) && !isModuleItem(item) ? 'bg-transparent' : undefined
const getIconSrc = (item: SearchResult) => {
  const icon = getIcon(item)
  return icon?.startsWith('data:') ? icon : 'data:image/png;base64,' + icon
}
</script>
