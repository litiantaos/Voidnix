<template>
  <div class="archive-view flex flex-col">
    <!-- 条目树：↑↓ 移动、→/回车 展开、← 收起、空格 预览；列表经 max-h 上限 + 内部滚动 -->
    <div v-if="truncated" class="text-xs text-muted px-3 pt-2" shrink="0">
      {{ t('archive.truncated', { n: entries.length }) }}
    </div>
    <div class="hide-scrollbar max-h-150 min-h-0 overflow-y-auto">
      <BaseList
        v-model:selected-index="index"
        :items="rows"
        :keyboard-active="true"
        id-field="id"
        @execute="onRowExecute"
      >
        <template #item="{ item }">
          <!-- 紧凑单行（镜像 proxy 连接列表范式）：p-3 text-xs，名称 flex-1 + 大小右对齐 -->
          <div p="3" flex gap="2" text="xs" select="none" items="center">
            <i
              :class="item.icon"
              class="shrink-0"
              :style="{ marginLeft: `${item.depth * 14}px` }"
            ></i>
            <span flex="1" min-w="0" truncate :title="item.path">{{ item.title }}</span>
            <span v-if="item.isDir && item.children?.length" class="text-muted shrink-0">{{
              item.children.length
            }}</span>
            <span v-else class="text-muted shrink-0 tabular-nums">{{ item.subtitle }}</span>
            <i
              v-if="item.isDir && item.children?.length"
              class="i-ri-arrow-right-s-line text-muted shrink-0 transition-transform"
              :class="expanded.has(item.id) ? 'rotate-90' : ''"
            ></i>
          </div>
        </template>
      </BaseList>
    </div>
  </div>

  <!-- 包内预览覆盖层（回车开/关、Esc 关；开启后 ↑↓ 切换跟随刷新）。
       与 clipboard 预览覆盖层同结构：纯内容居中、无信息栏（不挡内容） -->
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
        class="archive-preview-overlay hide-scrollbar"
        z="100"
        inset-0
        fixed
        bg="surface"
        overflow="auto"
      >
        <div flex items-center justify-center min-h="full" p="3">
          <template v-if="preview">
            <img
              v-if="preview.payload.type === 'image'"
              :src="`data:${preview.payload.mime};base64,${preview.payload.base64}`"
              max-w="full"
              max-h="full"
              object="contain"
              class="radius-ctrl"
              alt=""
            />
            <div
              v-else-if="preview.payload.type === 'text'"
              text="sm primary"
              leading="relaxed"
              whitespace="pre-wrap"
              wrap-anywhere
            >
              {{ preview.payload.content }}
            </div>
            <i v-else class="i-ri-file-unknow-line text-2xl text-muted opacity-50"></i>
          </template>
          <i v-else class="i-ri-loader-4-line text-2xl text-muted animate-spin"></i>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onDeactivated,
  onMounted,
  onUnmounted,
  ref,
  watch,
} from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseList from '@/components/ui/BaseList.vue'
import { onKeyStroke } from '@/composables/events'
import { isModalDialogOpen } from '@/utils/dom'
import { archiveInfo, archivePassword } from './index'
import { baseName, formatBytes, type EntryPreview } from './logic'

const appStore = useAppStore()

onMounted(() => document.addEventListener('keydown', onPreviewKey, true))
onUnmounted(() => document.removeEventListener('keydown', onPreviewKey, true))

/** 进入/重激活二级界面：聚焦顶部搜索框（MainView 的 subview watch 先聚焦滚动容器，
 * nextTick 后再聚焦输入框；KeepAlive 重激活无 onMounted 跟进，onActivated 补齐）。 */
function focusSearch() {
  if (!isViewActive()) return
  nextTick(() => {
    ;(document.getElementById('main-search-input') as HTMLInputElement | null)?.focus()
  })
}
onMounted(focusSearch)
onActivated(focusSearch)

/** 本视图是否活动（KeepAlive 下 document 监听切走后仍在，须自判归属）。 */
function isViewActive(): boolean {
  return appStore.activeExtId === 'archive' && appStore.activeSubview === 'entries'
}

// ── 树构建：扁平 entries → 嵌套节点 ──

interface TreeNode {
  /** 唯一键（归档内路径，目录去尾斜杠） */
  id: string
  name: string
  isDir: boolean
  size: number | null
  children: TreeNode[] | null
}

const entries = computed(() => archiveInfo.value?.entries ?? [])
const truncated = computed(() => archiveInfo.value?.truncated ?? false)

/** 树节点表：路径 → 节点（构建时补全中间目录——列表可能省略目录条目）。 */
function buildTree(): Map<string, TreeNode> {
  const nodes = new Map<string, TreeNode>()
  const ensureDir = (path: string): TreeNode => {
    const existing = nodes.get(path)
    if (existing) return existing
    const node: TreeNode = {
      id: path,
      name: baseName(path),
      isDir: true,
      size: null,
      children: [],
    }
    nodes.set(path, node)
    attach(node, path)
    return node
  }
  const attach = (node: TreeNode, path: string) => {
    const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : ''
    const parent = parentPath ? ensureDir(parentPath) : root
    if (parent.children) parent.children.push(node)
  }
  const root: TreeNode = { id: '', name: '', isDir: true, size: null, children: [] }
  for (const e of entries.value) {
    const isDir = e.kind === 'dir' || e.path.endsWith('/')
    const path = isDir ? e.path.replace(/\/+$/, '') : e.path
    if (nodes.has(path)) {
      if (!isDir && nodes.get(path)!.size === null) nodes.get(path)!.size = e.size
      continue
    }
    const node: TreeNode = {
      id: path,
      name: baseName(path),
      isDir,
      size: isDir ? null : e.size,
      children: isDir ? [] : null,
    }
    nodes.set(path, node)
    attach(node, path)
  }
  // 同名目录/文件排序：目录在前、名字典序（稳定呈现）
  const sortChildren = (n: TreeNode) => {
    if (!n.children) return
    n.children.sort((a, b) =>
      a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1,
    )
    n.children.forEach(sortChildren)
  }
  sortChildren(root)
  nodes.set('', root)
  return nodes
}

const treeNodes = computed(() => buildTree())

// ── 展开状态 + 可见行 ──

/** 展开的目录 id 集（首层默认展开） */
const expanded = ref(new Set<string>())

watch(
  entries,
  () => {
    const first = new Set<string>()
    for (const n of treeNodes.value.get('')?.children ?? []) {
      if (n.isDir) first.add(n.id)
    }
    expanded.value = first
  },
  { immediate: true },
)

interface Row {
  id: string
  title: string
  subtitle: string
  icon: string
  depth: number
  children: TreeNode[] | null
  isDir: boolean
  /** 归档内原始路径（预览用，目录条目带尾斜杠语义无差） */
  path: string
}

const rows = computed<Row[]>(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  const out: Row[] = []
  const makeRow = (c: TreeNode, depth: number): Row => ({
    id: c.id,
    title: c.name,
    subtitle: c.isDir ? '' : formatBytes(c.size),
    icon: c.isDir ? 'i-ri-folder-line' : 'i-ri-file-line',
    depth,
    children: c.children,
    isDir: c.isDir,
    path: c.id,
  })
  const walk = (node: TreeNode, depth: number) => {
    for (const c of node.children ?? []) {
      if (q) {
        // 搜索态：命中节点平铺（深度归零），未命中目录继续下钻找子孙
        if (c.id.toLowerCase().includes(q) || hasMatch(c)) {
          out.push(makeRow(c, 0))
        } else if (c.isDir && c.children?.length) {
          walk(c, depth + 1)
        }
        continue
      }
      out.push(makeRow(c, depth))
      if (c.isDir && c.children?.length && expanded.value.has(c.id)) walk(c, depth + 1)
    }
  }
  walk(treeNodes.value.get('') ?? ({ children: [] } as unknown as TreeNode), 0)
  return out
})

/** 子树内是否有节点命中搜索词。 */
function hasMatch(node: TreeNode): boolean {
  return (node.children ?? []).some(
    (c) => c.id.toLowerCase().includes(appStore.searchQuery.trim().toLowerCase()) || hasMatch(c),
  )
}

const index = ref(0)

function toggle(id: string) {
  const next = new Set(expanded.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expanded.value = next
}

/** 行回车：目录展开收起、文件开/关预览（空格已移除，回车唯一通道） */
function onRowExecute(row: Row) {
  if (row.isDir) {
    if (row.children?.length) toggle(row.id)
    return
  }
  previewOpen.value = !previewOpen.value
  if (!previewOpen.value) {
    preview.value = null
  }
}

// ── ←/→ 折叠展开（BaseList 只管 ↑↓/回车） ──

onKeyStroke(['ArrowLeft', 'ArrowRight'], (e) => {
  if (!isViewActive() || isModalDialogOpen()) return
  const row = rows.value[index.value]
  if (!row?.isDir || !row.children?.length) return
  e.preventDefault()
  const wantOpen = e.key === 'ArrowRight'
  if (expanded.value.has(row.id) !== wantOpen) toggle(row.id)
})

// ── 预览（空格开关，跟随高亮文件行） ──

const previewOpen = ref(false)
const preview = ref<EntryPreview | null>(null)
let previewToken = 0

function onPreviewKey(e: KeyboardEvent) {
  if (isModalDialogOpen() || !isViewActive()) return
  if (!previewOpen.value) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopImmediatePropagation()
    previewOpen.value = false
  }
}

watch([index, rows, previewOpen], async () => {
  const row = rows.value[index.value]
  if (!previewOpen.value || !row || row.isDir || !archiveInfo.value) {
    preview.value = null
    return
  }
  const token = ++previewToken
  preview.value = null
  try {
    const data = await invoke<EntryPreview>(CMD.archivePreview, {
      path: archiveInfo.value.path,
      entry: row.path,
      password: archivePassword.value || null,
    })
    if (token === previewToken) preview.value = data
  } catch {
    if (token === previewToken) preview.value = { payload: { type: 'binary' }, truncated: false }
  }
})

onDeactivated(() => {
  // 覆盖层经 Teleport 挂 body：不关会盖住目标界面，残留 previewOpen 会吞其它界面的 Esc
  previewOpen.value = false
  previewToken++
  // 离开二级界面清空过滤词（主表单的搜索框 readonly，残留文案无从清除会误导）
  appStore.setSearchQuery('')
})
</script>
