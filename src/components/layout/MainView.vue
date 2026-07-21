<template>
  <div
    flex="~ col"
    h="screen"
    w="screen"
    relative
    class="mica-shell"
    :class="{ 'mica-fog-run': fogRun }"
  >
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
    <!-- chrome 渐隐：浅蓝 canvas，栏中线以上不透明（theme.css .chrome-fade） -->
    <div class="chrome-fade" :style="chromeFadeStyle" aria-hidden="true" />
    <!--
      搜索栏拆层：毛玻璃底（backdrop-filter）与内容分离，
      避免 WKWebView 裁剪栏内按钮 box-shadow
    -->
    <div ref="searchBarRef" class="search-bar h-13 inset-x-3 top-3 absolute z-10">
      <div class="search-bar-surface acrylic-bar" aria-hidden="true" />
      <div class="search-bar-content px-3 flex gap-3 h-full min-w-0 items-center relative z-1">
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
          class="module-tag radius-ctrl"
          :class="{ 'is-hovered': isTagHovered }"
          @mouseenter="isTagHovered = true"
          @mouseleave="isTagHovered = false"
        >
          <span shrink="0" h="4" w="4" relative>
            <!-- 缩放交叉全在 theme.css（.module-tag.is-hovered），避免 ui-ctrl/Uno 抢 transition -->
            <span
              text="xs muted"
              h="3.5"
              w="3.5"
              inset="0"
              m="auto"
              absolute
              class="module-tag-icon flex-center"
              :class="activeModule.meta.icon"
              aria-hidden="true"
            />
            <BaseButton
              class="module-tag-close flex-center inset-0 absolute !p-0 !rounded-full !h-4 !w-4"
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
          min-w="0"
          :class="'placeholder:text-muted'"
          :placeholder="placeholderText"
          @input="onInput"
          @compositionstart="appStore.setComposing(true)"
          @compositionend="appStore.setComposing(false)"
        />

        <!-- 扩展附加区：勿 overflow-hidden，否则裁按钮阴影；data 供 Tab 圈定右侧控件 -->
        <div
          v-if="activeModule?.searchBarAccessory"
          ref="accessoryRef"
          data-search-bar-accessory
          flex
          gap="2"
          min-w="0"
          items="center"
          shrink="0"
        >
          <component :is="activeModule.searchBarAccessory()" />
        </div>

        <BaseButton
          v-if="updateStore.downloaded"
          icon="i-ri-arrow-up-circle-line text-accent"
          @click="updateStore.showDialog()"
        />
      </div>
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
import { getFocusableElements, cycleFocus, isFormControl } from '@/utils/dom'

const isDev = import.meta.env.DEV
const appStore = useAppStore()
const updateStore = useUpdateStore()

/** 窗壳雾：仅显示时播一轮，避免 blur 常驻动画占 GPU */
const fogRun = ref(false)
let fogClearTimer: ReturnType<typeof setTimeout> | null = null

function playFogOnce() {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
  fogRun.value = false
  nextTick(() => {
    fogRun.value = true
    if (fogClearTimer) clearTimeout(fogClearTimer)
    // 略长于最长动画 9.5s，卸 class 便于下次 show 重触发
    fogClearTimer = setTimeout(() => {
      fogRun.value = false
      fogClearTimer = null
    }, 10000)
  })
}

// chrome-fade 高度覆盖（配方在 theme.css .chrome-fade）
const chromeFadeStyle = {
  '--chrome-fade-height': `${WINDOW.CHROME_FADE_HEIGHT}px`,
} as Record<string, string>

const searchBarRef = ref<HTMLDivElement>()
const accessoryRef = ref<HTMLDivElement>()
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

// Tab 环：搜索框 + 右侧附加控件（顺序循环，不含 module-tag 关闭）。
//
// 主界面：始终锁在环内——内容区 BaseTextarea 等 tabindex=0 不得抢 Tab。
// 子视图：进入时 watch 会把焦点落到 scrollContainer（防失焦藏窗）；此时仍锁环，
//   避免「一切设置就 Tab 进内容区」。仅当用户已 Enter/点进设置控件
//   （data-settings-control 等）时放行，让表单内 Tab 自管。
// 对话框：完全自管。
function onTabKeydown(e: KeyboardEvent) {
  if (e.key !== 'Tab') return
  const mod = activeModule.value
  if (!mod?.searchBarAccessory) return
  if (appStore.isDialogOpen) return

  const active = document.activeElement as HTMLElement | null
  const inBar = !!searchBarRef.value?.contains(active)

  if (appStore.activeSubview) {
    // 焦点已在设置表单控件 → 放行；在栏内 / scroll 容器 / body → 继续锁环
    const inSettingsControl =
      !!active &&
      !inBar &&
      active !== document.body &&
      isFormControl(active, { settingsControl: true })
    if (inSettingsControl) return
  }

  const search = searchInput.value
  const right = accessoryRef.value ? getFocusableElements(accessoryRef.value) : []
  const ring = mod.disableSearchInput || !search ? right : [search, ...right]
  if (ring.length === 0) return
  e.preventDefault()
  e.stopPropagation()
  cycleFocus(ring, e)
}

onMounted(() => {
  // capture：先于内容区控件默认 Tab 行为
  document.addEventListener('keydown', onTabKeydown, true)
  // 启动时若窗已可见，动一轮；之后每次获焦再动
  playFogOnce()
  window.addEventListener('window-focused', playFogOnce)
})
onUnmounted(() => {
  document.removeEventListener('keydown', onTabKeydown, true)
  window.removeEventListener('window-focused', playFogOnce)
  if (fogClearTimer) {
    clearTimeout(fogClearTimer)
    fogClearTimer = null
  }
})

function onTagClose() {
  isTagHovered.value = false
  handleTagClose()
}
</script>
