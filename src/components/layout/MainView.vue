<template>
  <div flex="~ col" h="screen" w="screen" relative class="mica-shell">
    <div
      v-if="isDev"
      bg="accent"
      h="1"
      w="8"
      left="1/2"
      top="1"
      absolute
      class="z-20 -translate-x-1/2"
    />
    <!-- chrome 渐隐遮罩：材质抽离为 .chrome-fade（theme.css），高度走 CSS 变量 -->
    <div class="chrome-fade" :style="chromeFadeStyle" aria-hidden="true" />
    <!-- 搜索栏：acrylic-bar = acrylic + glass-ring + radius-panel + border -->
    <div
      ref="searchBarRef"
      px="3"
      flex
      gap="3"
      h="13"
      items="center"
      class="acrylic-bar inset-x-3 top-3 absolute z-10"
    >
      <!-- 扩展标签 -->
      <div
        v-if="activeModule"
        text="xs secondary"
        p="x-3"
        flex="~ none"
        gap="1.5"
        h="7"
        select="none"
        items="center"
        class="radius-ctrl fill-ctrl"
        @mouseenter="isTagHovered = true"
        @mouseleave="isTagHovered = false"
      >
        <!-- 图标缩小 / 关闭钮放大交叉：进 spring 回弹，出 ease-in 快收；叠在 fill-ctrl 标签上故底色深一档 -->
        <span shrink="0" h="3.5" w="3.5" relative>
          <span
            text="xs muted"
            h="3.5"
            w="3.5"
            inset="0"
            absolute
            class="flex-center transition-all"
            :class="[
              activeModule.meta.icon,
              isTagHovered
                ? 'opacity-0 scale-50 duration-100 ease-in'
                : 'opacity-100 scale-100 duration-200 ease-spring',
            ]"
            aria-hidden="true"
          />
          <BaseButton
            class="rounded-full flex-center transition-all inset-0 absolute !p-0 !bg-black/8 !h-3.5 !w-3.5 hover:!bg-black/12"
            :class="
              isTagHovered
                ? 'opacity-100 scale-100 duration-200 ease-spring'
                : 'opacity-0 scale-50 duration-100 ease-in pointer-events-none'
            "
            :tabindex="isTagHovered ? undefined : -1"
            icon="i-ri-close-line text-xs text-secondary"
            @click="onTagClose"
          />
        </span>
        <span>{{ activeModule.meta.name }}</span>
      </div>

      <input
        ref="searchInput"
        id="main-search-input"
        data-list-execute
        :value="appStore.searchQuery"
        :readonly="activeModule?.disableSearchInput"
        text="base primary"
        outline="none"
        bg="transparent"
        flex="1"
        :class="'placeholder:text-muted'"
        :placeholder="placeholderText"
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
      :group-field="getGroupKey"
      :group-title="groupTitle"
      @update:selected-index="(i: number) => (selectedIndex = i)"
    />
  </div>

  <UpdateDialog v-if="updateStore.dialogVisible" @close="updateStore.closeDialog()" />

  <ResultActionPanel :results="results" :selected-index="selectedIndex" />
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useScroll } from '@/composables/events'
import { getExtension } from '@/runtime/extension-registry'
import { SEARCH, WINDOW } from '@/runtime/constants'
import { getGroupKey } from '@/runtime/search-engine'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import type { SearchResult } from '@/runtime/types'
import ContentView from '@/components/layout/ContentView.vue'
import ResultActionPanel from '@/components/layout/ResultActionPanel.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import UpdateDialog from '@/components/ui/UpdateDialog.vue'

import { useScrollPosition } from '@/composables/useScrollPosition'
import { useSearchInput } from '@/composables/useSearchInput'
import { useResultNavigation } from '@/composables/useResultNavigation'
import { useModuleHeight } from '@/composables/useModuleHeight'
import { getFocusableElements, cycleFocus } from '@/utils/dom'

const isDev = import.meta.env.DEV
const appStore = useAppStore()
const updateStore = useUpdateStore()

// chrome-fade 高度覆盖（配方在 theme.css .chrome-fade）
const chromeFadeStyle = {
  '--chrome-fade-height': `${WINDOW.CHROME_FADE_HEIGHT}px`,
} as Record<string, string>

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

// placeholder：搜索说明
// disableSearchInput 模块仅当显式声明 placeholder 才显示（如 uuid），否则空
// 子视图激活时：用「搜索{subviewTitle}」（扩展声明 subviewTitle 映射）
const placeholderText = computed(() => {
  const mod = activeModule.value
  if (!mod) {
    return '搜索应用、文件、扩展等'
  }
  const subview = appStore.activeSubview
  if (subview && mod.subviewTitle?.[subview]) {
    return `搜索${mod.subviewTitle[subview]}`
  }
  return mod.placeholder ?? (mod.disableSearchInput ? '' : `在 ${mod.meta.name} 中搜索`)
})

// 分组逻辑：getGroupKey 单一源（search-engine）；标题读 constants 单一源
const groupTitle = (group: string) =>
  SEARCH.GROUP_TITLES[group as keyof typeof SEARCH.GROUP_TITLES] || group

const {
  onInput,
  handleTagClose,
  isLoading,
  clearSearch,
  loadDefaultResults,
  activateModule,
  goHome,
  exitModule,
} = useSearchInput({
  searchInput,
  results,
  selectedIndex,
  activeModule,
  restore,
  reset,
})
const { handleExecute } = useResultNavigation({
  results,
  selectedIndex,
  activeModule,
  clearSearch,
  loadDefaultResults,
  activateModule,
  goHome,
  exitModule,
})

useModuleHeight({
  activeModule,
  activeSubview: computed(() => appStore.activeSubview),
  contentRef: computed(() => contentViewRef.value?.contentRef),
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
// 从主列表进入时保存其滚动位置，exitModule 调用 restore('tools') 恢复
watch(
  () => activeModule.value?.meta.id,
  (newId, oldId) => {
    if (newId && newId !== oldId) {
      if (!oldId) save('tools')
      reset()
      // disableSearchInput 模块用 readonly（始终可聚焦）而非 disabled，
      // 离开时 focus 不受 disabled→enabled 同帧缓存影响；
      // 进入时主动 blur 避免残留光标（与原 disabled 行为一致）
      if (getExtension(newId)?.disableSearchInput) searchInput.value?.blur()
    } else if (!newId && oldId) {
      nextTick(() => searchInput.value?.focus())
    }
  },
)

// Tab 切换搜索栏附加区：监听上提到 document 级，
// 进入 disableSearchInput 模块时 input 被 blur 到 body（非 searchBarRef 子节点），
// searchBarRef 上的 keydown 捕获不到，故需 document 级才能一次命中。
// - 搜索栏禁用：跳过 readonly 搜索框，Tab 仅在附加区控件间循环续切
// - 焦点已在附加区某控件：基于当前位置前进到下一个（非从头开始）
function onTabKeydown(e: KeyboardEvent) {
  if (e.key !== 'Tab') return
  const mod = activeModule.value
  if (!mod?.searchBarAccessory) return
  // 对话框/子视图打开时交由它们自管焦点
  if (appStore.isDialogOpen || appStore.activeSubview) return
  const active = document.activeElement
  const inBar = !!searchBarRef.value?.contains(active)
  // 仅当焦点在搜索栏内或落在 body（失焦态）时接管，避免抢夺内容区/子视图焦点
  if (!inBar && active !== document.body) return
  const focusable = getFocusableElements(searchBarRef.value!).filter(
    (el) => !mod.disableSearchInput || el !== searchInput.value,
  )
  if (focusable.length === 0) return
  e.preventDefault()
  cycleFocus(focusable, e)
}

onMounted(() => document.addEventListener('keydown', onTabKeydown))
onUnmounted(() => document.removeEventListener('keydown', onTabKeydown))

function onTagClose() {
  isTagHovered.value = false
  handleTagClose()
}
</script>
