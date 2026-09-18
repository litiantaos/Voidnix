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
        :style="mergedContentStyle"
      >
        <!-- max=3：日常高频扩展（agent/settings/proxy）不超过 3 个同时活跃，超出按 LRU 驱逐。
             KeepAlive 常驻（v-if 下沉到动态组件）：切换/往返/窗口隐藏唤起一律保留视图
             状态（选中/滚动/会话）——隐藏时的 layer 释放由 contentRef 的
             content-visibility:hidden 承担（clearCache），不卸载 DOM。 -->
        <KeepAlive :max="3">
          <component
            v-if="resolvedView"
            :is="resolvedView"
            :key="`${props.extension?.meta.id ?? 'main'}-${appStore.activeSubview ?? 'view'}`"
          />
        </KeepAlive>

        <!-- Standard list -->
        <BaseList
          v-if="!resolvedView && props.results.length > 0"
          :items="props.results"
          :selected-index="props.selectedIndex"
          :multi-select="isMultiSelect"
          :selected-ids="selectedIds"
          :keyboard-active="!!appStore.activeExtId"
          :composing="appStore.isComposing"
          :action-hint="hasActionMenu"
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
        :title="t('common.loading')"
      />
      <BaseEmptyState
        v-else-if="!resolvedView && props.results.length === 0"
        :title="t('common.noResults')"
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
import { t } from '@/runtime/i18n'
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

/** 支持右键动作菜单（ResultActionPanel）的结果才显示快捷键提示，与面板 canOpen 同条件：
 *  仅全局模式（扩展模式下 Cmd+Enter 走 execute 的 reveal 直达，不开面板） */
function hasActionMenu(result: SearchResult): boolean {
  if (appStore.activeExtId) return false
  const kind = result.data?.kind
  return !!result.data?.path && (kind === 'application' || kind === 'file' || kind === 'folder')
}

// 窗口隐藏时 toggle content-visibility:hidden：跳过 contentRef 全子树渲染
// （标准列表 + 扩展视图），forced layout 释放 compositing layer tile backing。
// DOM 保留不闪烁；show 时 compositor 同步处理 pending 的 visible 变更，首帧即可见。
// 不卸载 KeepAlive：窗口隐藏唤起是无缝继续（任何列表的滚动/选中/会话状态冻结），
// 卸载重建会清空扩展视图状态。
const contentHidden = ref(false)

async function clearCache() {
  // scrollTop 兜底回填：hidden 帧 forced layout 期间若引擎未按 last remembered size
  // 保留高度（实现差异），scroll 容器内容收缩被 clamp 且恢复后无人回填。常驻声明
  // contain-intrinsic-size 已根治记录问题，此处兜底 WKWebView 等实现差异，零成本。
  const sc = scrollContainer.value
  const savedTop = sc?.scrollTop ?? 0
  contentHidden.value = true
  await nextTick()
  // 强制同步 layout：让 WebCore 在 alpha=0 窗口完成 content-visibility:hidden
  // 子树（结果列表 + 扩展视图）的 compositing layer tile backing 释放。
  void document.body.offsetHeight
  contentHidden.value = false
  await nextTick()
  if (sc && savedTop > 0 && sc.scrollTop !== savedTop) sc.scrollTop = savedTop
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

/** contentStyle + 隐藏态 content-visibility 叠加。
 *  content-visibility:hidden 跳过子树渲染，forced layout 释放 tile backing（IOSurface）。
 *  contain-intrinsic-size 常驻声明（未隐藏帧同样生效）：last remembered size 仅在
 *  「已声明 auto 值且正常渲染」的帧记录——若只与 hidden 同帧声明则从未记录，skip 时
 *  高度回退兜底值，forced layout 使 scroll 容器 clamp scrollTop（唤起后滚动归顶根因）。
 *  常驻声明下 skip 占位 = 上次真实尺寸，内容高度不变则滚动天然保留；
 *  未 skip 时无 size containment 配合，该属性对布局零副作用。 */
const mergedContentStyle = computed(() => {
  const style = {
    ...(contentStyle.value ?? {}),
    containIntrinsicSize: `auto ${WINDOW.DEFAULT_HEIGHT - WINDOW.CHROME_HEIGHT}px`,
  }
  if (!contentHidden.value) return style
  return { ...style, contentVisibility: 'hidden' as const }
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
