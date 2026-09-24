<template>
  <!-- 恒在包裹层：无 contain（BaseList 根的 contain:layout 会使内容高度不参与
       撑开滚动容器），空态与列表共存其下；BaseList 常挂（v-show）——v-if 空态
       切换会卸载 BaseList，其 activeExtId 归零 watch 在卸载期间缺席、重挂载不
       触发 activated（inKeepAliveTree 停 false），跨会话归零旁路；常挂后归零
       职责完全回归 BaseList 统一语义 -->
  <div>
    <BaseEmptyState
      v-if="history.length === 0"
      class="h-full"
      :title="t('clipboard.empty')"
      icon="i-ri-clipboard-line"
      :loading="loading"
    />

    <BaseList
      v-show="history.length > 0"
      ref="listRef"
      :items="history"
      :selected-index="selectedIndex"
      multi-select
      :selected-ids="selectedIds"
      :keyboard-active="!open && !previewOpen && !editOpen"
      id-field="id"
      action-hint
      @update:selected-ids="selectedIds = $event"
      @select="selectedIndex = $event"
      @execute="handleExecute"
      @contextmenu="toggleOpen"
    >
      <template #item="{ item }">
        <BaseListItem :ref="(el: unknown) => setImageRef(el, item)" multiline-title>
          <template #icon>
            <div
              v-if="getColor(item)"
              rounded
              h="4"
              w="4"
              :style="{ backgroundColor: getColor(item)! }"
            ></div>
            <i
              v-else-if="item.content_type === 'text'"
              class="i-ri-t-box-line text-sm text-accent"
            ></i>
            <i
              v-else-if="item.content_type === 'image'"
              class="i-ri-image-line text-sm text-success"
            ></i>
            <i v-else class="i-ri-folder-3-line text-sm text-warning"></i>
          </template>
          <template #title>
            <div
              v-if="item.content_type === 'text'"
              whitespace-pre-wrap
              wrap-break-word
              line-clamp="5"
            >
              {{ item.content }}
            </div>
            <img
              v-else-if="item.content_type === 'image' && imageCache.get(item.id)"
              :src="imageCache.get(item.id)"
              class="border border-divider radius-ctrl border-solid fill-ctrl"
              h="32"
              w="48"
              object="cover top"
              loading="lazy"
              :alt="t('clipboard.imageAlt')"
            />
            <!-- 缩略图占位（仅加载中态）：与 img 同尺寸同边框（Wind4 preflight 将 img
                 reset 为 block，占位同为块级精确等高）。无占位时图片项走下方文本回退
                 （矮一行），懒加载完成后条目变高，已滚动到位的选中项（如按上键 wrap
                 到末项）被推出视口；加载失败（图片文件被清理 / invoke 异常）记入
                 failedImages 回落文本分支，不滞留空白块 -->
            <div
              v-else-if="item.content_type === 'image' && !failedImages.has(item.id)"
              class="border border-divider radius-ctrl border-solid fill-ctrl"
              h="32"
              w="48"
            ></div>
            <div v-else truncate>
              {{ item.content.split('/').filter(Boolean).pop() || item.content }}
            </div>
          </template>
          <template #subtitle>
            <div flex gap="1.5" items="center">
              <span>{{ item.source_app }}</span>
              <span text="muted">·</span>
              <span>{{ formatClipboardTime(item.created_at) }}</span>
              <template v-if="item.file_size">
                <span text="muted">·</span>
                <span>{{ formatBytes(item.file_size) }}</span>
              </template>
              <template v-if="item.image_width && item.image_height">
                <span text="muted">·</span>
                <span>{{ item.image_width }}×{{ item.image_height }}</span>
              </template>
              <!-- 收藏标记：副标题末尾点分隔小星，色与尺寸随副标题文字继承 -->
              <template v-if="item.is_favorite">
                <span text="muted">·</span>
                <i class="i-ri-star-line"></i>
              </template>
            </div>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>

  <!-- Cmd+回车 动作菜单（界面右下角，同下拉框样式，键盘可达）-->
  <Teleport to="body">
    <Transition name="ui-popup" appear>
      <div
        v-if="open"
        ref="panelRef"
        tabindex="-1"
        class="dropdown-panel outline-none bottom-3 right-3 fixed z-50"
        role="menu"
      >
        <BaseDropdownItems
          :items="actionMenuItems"
          :active-index="menuIndex"
          @select="onMenuClick"
          @hover="(i: number) => (menuIndex = i)"
        />
      </div>
    </Transition>
  </Teleport>

  <!-- 预览覆盖层（Esc 关闭）-->
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-[var(--duration-fast)] ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-[var(--duration-fastest)] ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="previewOpen"
        class="hide-scrollbar"
        z="100"
        inset-0
        fixed
        bg="surface"
        overflow="auto"
      >
        <div flex items-center justify-center min-h="full" p="3">
          <img
            v-if="previewType === 'image' && previewImage"
            :src="previewImage"
            max-w="full"
            max-h="full"
            object="contain"
            class="radius-ctrl"
            :alt="t('clipboard.previewImageAlt')"
          />
          <span
            v-else-if="previewType === 'image'"
            class="i-ri-loader-4-line text-2xl text-muted animate-spin"
          />
          <!-- anywhere（非 break-word）参与 min-content 计算：flex 居中容器下长 URL 才能收缩换行，否则溢出窗口 -->
          <div v-else text="sm primary" leading="relaxed" whitespace="pre-wrap" wrap-anywhere>
            {{ previewText }}
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>

  <!-- 编辑弹窗（仅文本）-->
  <BaseDialog
    v-if="editOpen"
    :title="t('clipboard.editTitle')"
    variant="form"
    size="md"
    show-footer
    :ok-label="t('common.save')"
    @confirm="saveEdit"
    @cancel="editOpen = false"
  >
    <div class="form-field">
      <BaseTextarea
        v-model="editText"
        :rows="12"
        :max-height="0"
        :auto-resize="false"
        :submit-on-enter="false"
        :placeholder="t('clipboard.editPlaceholder')"
      />
    </div>
  </BaseDialog>
</template>

<script setup lang="ts">
import {
  ref,
  computed,
  onActivated,
  onDeactivated,
  onMounted,
  onUnmounted,
  watch,
  shallowReactive,
} from 'vue'
import {
  history,
  activeTab,
  activeType,
  loading,
  fetchClipboardHistory,
  invalidateCache,
} from './index'
import type { ClipboardItem } from './index'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseDropdownItems, { type PanelItem } from '@/components/ui/BaseDropdownItems.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import { useActionPanel } from '@/composables/useActionPanel'
import { useAppStore } from '@/stores/app'
import { formatBytes, toErrorMessage } from '@/utils/format'
import { formatClipboardTime } from './logic'
import { t } from '@/runtime/i18n'

const appStore = useAppStore()

// BaseList 实例（归零经组件契约 reset()：瞬时落位 + 滚顶，见 BaseList）。
// 结构化类型只声明消费方法：泛型组件（generic="T"）无法经 InstanceType 提取实例类型
const listRef = ref<{ reset: () => void } | null>(null)

const selectedIds = ref(new Set<string>())
const selectedIndex = ref(0)

/// 归零 + 重拉（会话复位 / 过滤条件变化共用）：列表是动态置顶序，内容全变时保留
/// 索引指向已漂移的记录，统一归首项。View 停用期间同样生效（后台刷新，重挂载
/// 从首项起）
function resetAndRefetch(query: string, favorites: boolean) {
  selectedIds.value = new Set()
  if (listRef.value) listRef.value.reset()
  else selectedIndex.value = 0
  fetchClipboardHistory(query, favorites)
}

let debounceTimer: ReturnType<typeof setTimeout>
watch([activeTab, activeType, () => appStore.searchQuery], ([tab, , query]) => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    // 过滤条件变化 = 列表重新过滤：内容全变，选中归首项（保留索引指向已漂移的记录）
    resetAndRefetch(query, tab === 'favorites')
  }, 80)
})

// history 替换后越界 clamp（删除 / favorites tab 下取消收藏使列表缩短）：
// 越界 localIndex 无高亮（方向键 wrapIndex 才自愈），收敛为贴尾——连续删除不打断；
// 空列表 clamp 至 0，回填重挂载从首项起；过滤路径已归零不触发
watch(history, (items) => {
  if (selectedIndex.value >= items.length) {
    selectedIndex.value = Math.max(0, items.length - 1)
  }
})

watch(
  () => appStore.activeExtId,
  (id) => {
    if (id !== 'clipboard') {
      clearTimeout(debounceTimer)
      selectedIds.value = new Set()
      // 选中归零由 BaseList 统一承载（watch activeExtId 跨会话转移归零；BaseList
      // 常挂不卸载，归零 watch 恒在）；此处不重置：subview（config）往返也触发
      // onActivated，重置会破坏往返保留语义
    }
  },
)

// 会话结束即归位（消除唤起首帧残影——hide 不 orderOut 架构下 show 立即可见的是
// 隐藏前最后一帧，唤起侧才归零会让旧选中位置可见几十 ms）：窗口隐藏时归位，
// 覆盖全部前端隐藏路径（blur/主快捷键再按/Esc/click-outside 等均经 hideWindow
// 派发 window-hiding）；列表是动态置顶序（系统复制插入顶部、粘贴刷新被贴记录时间
// 置顶），保留的选中索引跨会话指向已漂移的记录——BaseList 默认的「窗口唤起保留」三态语义在
// 剪贴板上按 View 数据语义覆盖。过滤词原样保留（窗口显隐不改扩展内容状态）。
// 延迟一档宏任务：ContentView clearCache 同事件内保存/回填 scrollTop（兜底
// content-visibility 帧的引擎 clamp，纯微任务链），归位直写 scrollTop 会被其
// savedTop 回填覆盖，宏任务边界稳定后行
function onWindowHiding() {
  if (appStore.activeExtId !== 'clipboard') return
  const query = appStore.searchQuery
  const favorites = activeTab.value === 'favorites'
  setTimeout(() => resetAndRefetch(query, favorites), 0)
}

async function handleExecute(item: ClipboardItem, _index: number, _e?: KeyboardEvent) {
  // Cmd+回车开菜单由捕获相监听拦截（避免 BaseList 清空多选）；此处仅处理粘贴（回车/双击）
  const ids = selectedIds.value.size > 0 ? [...selectedIds.value] : [item.id].filter(Boolean)
  selectedIds.value = new Set()
  if (ids.length === 0) return
  try {
    if (ids.length > 1) {
      await invoke(CMD.pasteClipboardItems, { ids })
    } else {
      await invoke(CMD.pasteClipboardItem, { id: ids[0] })
    }
    invalidateCache()
    // 粘贴成功即会话结束：invoke 返回时窗口已被 Rust 端 hide_main 隐藏（不经前端
    // hideWindow、无 window-hiding 事件），此刻归位使 DOM 更新在隐藏期完成，
    // 下次唤起首帧即首项；失败路径（窗口未隐藏）保留选中供重试
    listRef.value?.reset()
  } catch (e) {
    console.error('Failed to paste clipboard:', e)
    appStore.showStatus(toErrorMessage(e, t('clipboard.pasteFailed')), {
      kind: 'error',
      duration: 4000,
    })
  }
}

// ── 动作菜单（Cmd+回车，界面右下角，键盘可达）──
// 键盘导航 / 外点关闭 / Cmd+Enter 打开拦截 由 useActionPanel 统一承载
const panelRef = ref<HTMLElement>()
const menuTarget = ref<ClipboardItem | null>(null)
const menuBatch = ref(false)

const actionMenuItems = computed<PanelItem[]>(() => {
  if (menuBatch.value) {
    return [
      {
        type: 'item',
        key: 'delete',
        label: t('clipboard.deleteBatch', { count: selectedIds.value.size }),
        icon: 'i-ri-delete-bin-line',
        danger: true,
      },
    ]
  }
  const item = menuTarget.value
  if (!item) return []
  const isText = item.content_type === 'text'
  const items: PanelItem[] = []
  if (item.content_type !== 'file') {
    items.push({
      type: 'item',
      key: 'preview',
      label: t('clipboard.preview'),
      icon: 'i-ri-eye-line',
    })
  }
  items.push({
    type: 'item',
    key: 'favorite',
    label: item.is_favorite ? t('clipboard.unfavorite') : t('clipboard.favorite'),
    icon: item.is_favorite ? 'i-ri-star-fill text-warning' : 'i-ri-star-line',
  })
  if (isText) {
    items.push({ type: 'item', key: 'edit', label: t('clipboard.edit'), icon: 'i-ri-edit-line' })
  }
  items.push({
    type: 'item',
    key: 'delete',
    label: t('common.delete'),
    icon: 'i-ri-delete-bin-line',
    danger: true,
  })
  return items
})

const { open, menuIndex, close, toggleOpen, onMenuClick } = useActionPanel({
  panelRef,
  getItems: () => actionMenuItems.value,
  onSelect: runMenuAction,
  canOpen: prepareMenu,
})

/// 准备菜单目标（Cmd+Enter 与右键共用）：多选 → 批量删除；否则 → 当前项完整菜单
function prepareMenu(): boolean {
  if (appStore.activeExtId !== 'clipboard') return false
  if (previewOpen.value || editOpen.value) return false
  if (selectedIds.value.size > 0) {
    menuBatch.value = true
    menuTarget.value = null
    return true
  }
  const item = history.value[selectedIndex.value]
  if (!item) return false
  menuBatch.value = false
  menuTarget.value = item
  return true
}

function runMenuAction(key: string | number) {
  if (menuBatch.value) {
    if (key !== 'delete') return
    const ids = [...selectedIds.value]
    close()
    void deleteItems(ids)
    return
  }
  const target = menuTarget.value
  if (!target) return
  close()
  switch (key) {
    case 'favorite':
      void toggleFavorite(target.id)
      break
    case 'preview':
      void openPreview(target)
      break
    case 'edit':
      void openEdit(target)
      break
    case 'delete':
      void deleteItems([target.id])
      break
  }
}

// 预览覆盖层 Esc 关闭（独立捕获相监听；preview 与菜单互斥，菜单由 composable 处理）
function onPreviewKey(e: KeyboardEvent) {
  if (!previewOpen.value) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopImmediatePropagation()
    previewOpen.value = false
  }
}
onMounted(() => {
  document.addEventListener('keydown', onPreviewKey, true)
  window.addEventListener('window-hiding', onWindowHiding)
})
onUnmounted(() => {
  document.removeEventListener('keydown', onPreviewKey, true)
  window.removeEventListener('window-hiding', onWindowHiding)
})

const toggleFavorite = async (id: string) => {
  try {
    await invoke(CMD.toggleClipboardFavorite, { id })
    history.value = history.value.map((i) =>
      i.id === id ? { ...i, is_favorite: !i.is_favorite } : i,
    )
    if (activeTab.value === 'favorites') {
      history.value = history.value.filter((i) => i.is_favorite)
    }
    invalidateCache()
  } catch (e) {
    console.error('Failed to toggle favorite:', e)
  }
}

// ── 预览 ──
const previewOpen = ref(false)
const previewType = ref<'text' | 'image'>('text')
const previewText = ref('')
const previewImage = ref('')

async function openPreview(item: ClipboardItem) {
  if (item.content_type === 'image') {
    previewType.value = 'image'
    previewImage.value = ''
    previewOpen.value = true
    try {
      const data = await invoke<string | null>(CMD.getClipboardImage, { id: item.id })
      if (data) previewImage.value = data
    } catch (e) {
      console.error('Failed to load image:', e)
    }
    return
  }
  previewType.value = 'text'
  previewText.value = t('clipboard.loadingText')
  previewOpen.value = true
  try {
    previewText.value = (await invoke<string | null>(CMD.getClipboardText, { id: item.id })) ?? ''
  } catch (e) {
    console.error('Failed to load text:', e)
    previewText.value = t('clipboard.loadFailed')
  }
}

// ── 编辑（仅文本）──
const editOpen = ref(false)
const editText = ref('')
const editingId = ref('')

async function openEdit(item: ClipboardItem) {
  editingId.value = item.id
  editText.value = t('clipboard.loadingText')
  editOpen.value = true
  try {
    editText.value = (await invoke<string | null>(CMD.getClipboardText, { id: item.id })) ?? ''
  } catch (e) {
    console.error('Failed to load text:', e)
    editText.value = ''
  }
}

async function saveEdit() {
  if (!editingId.value) return
  try {
    await invoke(CMD.updateClipboardText, { id: editingId.value, content: editText.value })
    invalidateCache()
    await fetchClipboardHistory(appStore.searchQuery, activeTab.value === 'favorites')
  } catch (e) {
    console.error('Failed to update text:', e)
  }
  editOpen.value = false
}

// ── 删除 ──
async function deleteItems(ids: string[]) {
  if (ids.length === 0) return
  const confirmed = await appStore.showConfirm({
    title: t('clipboard.deleteTitle'),
    message:
      ids.length > 1
        ? t('clipboard.deleteConfirmMessageMulti', { count: ids.length })
        : t('clipboard.deleteConfirmMessageSingle'),
    okLabel: t('common.delete'),
    cancelLabel: t('common.cancel'),
  })
  if (!confirmed) return
  try {
    await invoke(CMD.deleteClipboardItems, { ids })
    invalidateCache()
    selectedIds.value = new Set()
    await fetchClipboardHistory(appStore.searchQuery, activeTab.value === 'favorites')
  } catch (e) {
    console.error('Failed to delete clipboard item:', e)
  }
}

onActivated(() => {
  activeTab.value = 'all'
  activeType.value = 'all'
  // 选中归首项由 BaseList 统一承载（watch activeExtId 跨会话转移归零）；
  // 此处不重置：subview（config）往返也触发 onActivated，重置会破坏往返保留语义
  fetchClipboardHistory('', false)
})

onDeactivated(() => {
  // 离开扩展时清空 UI 辅助缓存，避免长期驻留增长（重新进入时自然重建）。
  // 不清 tabCache/history：tabCache 经 previewOnly 截断（~200 字符/条），
  // 重新进入直接命中缓存秒出，避免「空列表 → 异步 IPC → 补全」的空闪。
  colorCache.clear()
  failedImages.clear()
  // 预览覆盖层经 Teleport 挂 body（不随宿主 deactivate 移除）：不关会盖住目标界面，
  // 且残留 previewOpen 使 onPreviewKey 的 capture Esc 在其它界面被吞（首按只静默关预览）
  previewOpen.value = false
})

// ── 图片懒加载 ──
const IMAGE_CACHE_MAX = 30
const imageCache = shallowReactive(new Map<string, string>())
const pendingImages = new Set<string>()
// 加载失败（Rust 返回 null——图片文件已被外部清理，或 invoke reject）的条目集合：
// 响应式驱动模板回落文本分支，占位仅覆盖加载中态
const failedImages = shallowReactive(new Set<string>())

function cacheImage(id: string, data: string) {
  if (imageCache.size >= IMAGE_CACHE_MAX) {
    const first = imageCache.keys().next().value
    if (first) imageCache.delete(first)
  }
  imageCache.set(id, data)
}

const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue
      const id = (entry.target as HTMLElement).dataset.imageId
      if (id && !imageCache.has(id) && !pendingImages.has(id)) {
        pendingImages.add(id)
        invoke<string | null>(CMD.getClipboardImage, { id })
          .then((data) => {
            if (data) cacheImage(id, data)
            else failedImages.add(id)
            pendingImages.delete(id)
          })
          .catch(() => {
            failedImages.add(id)
            pendingImages.delete(id)
          })
      }
      observer.unobserve(entry.target)
    }
  },
  { rootMargin: '200px' },
)

function setImageRef(el: unknown, item: { id: string; content_type: string }) {
  if (item.content_type !== 'image') return
  if (imageCache.has(item.id) || pendingImages.has(item.id) || failedImages.has(item.id)) return
  const htmlEl = (el as { $el?: HTMLElement })?.$el ?? (el as HTMLElement | null)
  if (!htmlEl) return
  htmlEl.dataset.imageId = item.id
  observer.observe(htmlEl)
}

onUnmounted(() => observer.disconnect())

// ── 工具函数 ──
const COLOR_RE = /^(?:#[0-9a-fA-F]{3,8}|(?:rgb|hsl)a?\s*\([\d\s,%.\/]+\))$/
const colorCache = new Map<string, string | null>()

function getColor(item: { id: string; content_type: string; content: string }): string | null {
  if (item.content_type !== 'text') return null
  let cached = colorCache.get(item.id)
  if (cached === undefined) {
    const line = item.content.trim().split('\n')[0].trim()
    cached = COLOR_RE.test(line) ? line : null
    colorCache.set(item.id, cached)
  }
  return cached
}
</script>
