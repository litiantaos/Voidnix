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
      :style="{
        paddingTop: WINDOW.CHROME_HEIGHT + 'px',
        scrollPaddingTop: WINDOW.CHROME_HEIGHT + 'px',
        scrollPaddingBottom: WINDOW.CONTENT_INSET + 'px',
      }"
    >
      <!-- contentRef：量真实内容自然高（auto 高度模式消费）。
           fixed/default：min-height:100% + View 根 :deep flex-1 撑满可视区（声明值/DEFAULT_HEIGHT）。
           auto：View 根 :deep min-height = DEFAULT_HEIGHT - chrome（进入高度下限，撑满 loading/未配置空态；内容超出自然撑开驱动自适应）。
           列表/全局模式 contentStyle 返回 undefined，ContentView 空态作 scrollContainer 直接 flex 子项 flex-1 居中 -->
      <div
        ref="contentRef"
        :class="{ 'view-fixed': !isAutoHeight, 'view-auto': isAutoHeight }"
        flex="~ col"
        :style="contentStyle"
      >
        <!-- max=3：日常高频扩展（agent/settings/proxy）不超过 3 个同时活跃，超出按 LRU 驱逐。
             低频扩展重挂载毫秒级（KeepAlive activate/deactivate 语义已处理），换取 JS 堆削减。
             窗口隐藏时 keepAliveActive 置 false 卸载 KeepAlive 释放全部缓存 DOM + compositing layer。 -->
        <KeepAlive v-if="resolvedView && keepAliveActive" :max="3">
          <component
            :is="resolvedView"
            :key="`${props.extension?.meta.id ?? 'main'}-${appStore.activeSubview ?? 'view'}`"
          />
        </KeepAlive>

        <!-- Standard list -->
        <BaseList
          v-else-if="props.results.length > 0"
          :items="props.results"
          :selected-index="props.selectedIndex"
          :multi-select="isMultiSelect"
          :selected-ids="selectedIds"
          :keyboard-active="!!appStore.activeExtId"
          :composing="appStore.isComposing"
          @update:selected-ids="selectedIds = $event"
          :group-field="!extension ? props.groupField : undefined"
          :group-title="!extension ? props.groupTitle : undefined"
          @update:selected-index="(i: number) => emit('update:selectedIndex', i)"
          @contextmenu="() => emit('contextmenu')"
          @execute="handleExecute"
        >
          <template #item="{ item }">
            <ResultItem :item="item" :extension="extension" />
          </template>
        </BaseList>
      </div>

      <!-- 空态/加载态：scrollContainer 直接 flex 子项，flex-1 填满可视区垂直居中 -->
      <BaseEmptyState
        v-if="!resolvedView && props.loading && props.results.length === 0"
        :loading="true"
        title="加载中"
      />
      <BaseEmptyState
        v-else-if="!resolvedView && props.results.length === 0"
        title="无结果"
        :icon="extension ? extension.meta.icon : 'i-ri-search-line'"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { hideWindow } from '@/utils/tauri'
import { WINDOW } from '@/runtime/constants'
import type { Extension, SearchResult } from '@/runtime/types'
import BaseList from '@/components/ui/BaseList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import ResultItem from '@/components/layout/ResultItem.vue'

const props = defineProps<{
  extension?: Extension | null
  results: SearchResult[]
  loading?: boolean
  selectedIndex: number
  groupField?: (item: SearchResult) => string
  groupTitle?: (group: string) => string
  onExecute?: (result: SearchResult) => void
}>()

const emit = defineEmits<{
  'update:selectedIndex': [value: number]
  contextmenu: []
}>()

const appStore = useAppStore()

const selectedIds = ref(new Set<string>())
const isMultiSelect = computed(() => !!props.extension?.listOptions?.multiSelect)

// 窗口隐藏时置 false 卸载 KeepAlive（Vue onBeforeUnmount 全量释放缓存 vnode + compositing layer），
// nextTick 后置 true 重建空缓存（此时窗口即将 alpha=0，无视觉影响）。
const keepAliveActive = ref(true)

async function clearCache() {
  keepAliveActive.value = false
  await nextTick()
  // 强制同步 layout：处理 DOM removal 产生的 render tree 变更，
  // 让 WebCore 在 alpha=0 窗口仍完成 compositing layer 释放。
  // 同时覆盖 useSearchInput 清 results 的变更（Vue 批量 flush，两者一次 patch）。
  void document.body.offsetHeight
  keepAliveActive.value = true
}

onMounted(() => {
  window.addEventListener('window-hiding', clearCache)
})
onUnmounted(() => {
  window.removeEventListener('window-hiding', clearCache)
})

/**
 * 纯渲染器：布局决策收拢至此，搜索编排由 useSearchInput 统一承担（结果经 props 注入）。
 * subview 模式：当前扩展的私有命名子视图（screenshot{ocr}、各扩展{config}）。
 * mainView 模式：使用扩展声明的主视图。
 */
const resolvedView = computed(() => {
  const subviewId = appStore.activeSubview
  if (subviewId && props.extension?.subviews?.[subviewId]) {
    return props.extension.subviews[subviewId]()
  }
  return props.extension?.mainView?.()
})

// auto 高度模式（windowHeight/subviewHeight === 'auto'）：View 根保留 min-content（min-height:auto 默认），
// 内容自然高撑开 contentRef 驱动 ResizeObserver 自适应窗口；fixed/default 模式 View 根 min-height:0 撑满可视区
const isAutoHeight = computed(() => {
  const ext = props.extension
  if (!ext) return false
  const subId = appStore.activeSubview
  if (subId && ext.subviewHeights?.[subId] !== undefined) {
    return ext.subviewHeights[subId] === 'auto'
  }
  return ext.windowHeight === 'auto'
})

// fixed/default：contentRef min-height:100% 撑满可视区（View 根 :deep flex-1 撑满）。
// auto：contentRef 设 --content-min-h，View 根 :deep min-height = DEFAULT_HEIGHT - chrome
//      （进入高度下限，loading/未配置空态撑满；内容超出时 View 根自然撑开驱动窗口自适应）
const contentStyle = computed(() => {
  if (!resolvedView.value) return undefined
  if (isAutoHeight.value) {
    return { '--content-min-h': `${WINDOW.DEFAULT_HEIGHT - WINDOW.CHROME_HEIGHT}px` }
  }
  return { minHeight: '100%' }
})

const scrollContainer = ref<HTMLElement>()
const contentRef = ref<HTMLElement>()
defineExpose({ scrollContainer, contentRef })

const handleExecute = async (result: SearchResult, _index: number, e?: KeyboardEvent) => {
  // Cmd+Enter：在 Finder 中显示（reveal）
  if (e?.metaKey && result.data?.path) {
    await invoke(CMD.revealInFinder, { path: result.data.path })
    hideWindow()
    return
  }
  const multiResults =
    isMultiSelect.value && selectedIds.value.size > 0
      ? props.results.filter((r) => selectedIds.value.has(r.id))
      : undefined
  if (multiResults) selectedIds.value = new Set()
  if (props.onExecute) {
    props.onExecute(result)
  } else if (props.extension?.onExecute) {
    await props.extension.onExecute(result, multiResults)
  }
}
</script>

<style scoped>
/* 扩展 View 根自动撑满，消除手写 h-full 样板（新 View 零配置撑满）。
   - fixed/default（view-fixed）：flex:1 1 0% + min-height:0 撑满可视区，内部 overflow 滚动。
   - auto（view-auto）：min-height = DEFAULT_HEIGHT - chrome（进入高度下限），loading/空态撑满；
     内容超出时 min-height 不生效，View 根自然撑开驱动 ResizeObserver 自适应。
     用固定值不用百分比 —— 规避 WebKit「auto-height 容器 + flex 子项 + min-height:%」的循环依赖。 */
.view-auto > :deep(*) {
  min-height: var(--content-min-h);
}
.view-fixed > :deep(*) {
  flex: 1 1 0%;
  min-height: 0;
}
</style>
