<template>
  <div class="flex-col-view">
    <BaseSettingsList :items="filteredItems" :shortcut-id="FINDER_SHORTCUT.id" />

    <BaseDialog
      v-if="naming"
      :title="t('finderExt.newFile')"
      variant="form"
      size="sm"
      show-footer
      :ok-label="t('finderExt.create')"
      :close-on-confirm="false"
      @confirm="confirmNewFile"
      @cancel="cancelNaming"
    >
      <div class="form-field">
        <span class="form-label">{{ t('finderExt.fileName') }}</span>
        <BaseInput
          ref="nameInputRef"
          v-model="fileName"
          placeholder="Untitled.txt"
          @focus="onNameFocus"
        />
      </div>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onActivated, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import { t } from '@/runtime/i18n'
import { FINDER_SHORTCUT, FINDER_CATALOG } from './shortcuts'
import { runAction } from './actions'
import { entryViaShortcut, reactivateTick } from './index'

const appStore = useAppStore()
const { value: shortcutValue, update: updateShortcut } = useShortcutConfig(
  FINDER_SHORTCUT.id,
  FINDER_SHORTCUT.default,
)

const naming = ref(false)
const fileName = ref('Untitled.txt')
const nameInputRef = ref<InstanceType<typeof BaseInput> | null>(null)

function startNaming() {
  fileName.value = 'Untitled.txt'
  naming.value = true
}

function cancelNaming() {
  naming.value = false
}

/** 防 BaseDialog 回车 + 按钮重复触发 */
let submitting = false

async function confirmNewFile() {
  if (submitting) return
  const name = fileName.value.trim()
  if (!name) {
    appStore.showStatus(t('finderExt.emptyFileName'), { kind: 'error' })
    return
  }
  submitting = true
  try {
    const ok = await runAction('new_file', name)
    // closeOnConfirm=false：失败 toast 时弹窗保持；成功再卸窗（Rust 已 hide）
    if (ok) naming.value = false
  } finally {
    submitting = false
  }
}

/** 选中扩展名前的文件名（与访达重命名一致：Untitled.txt → 选中 Untitled） */
function selectFileStem(el: HTMLInputElement) {
  const v = el.value
  const dot = v.lastIndexOf('.')
  // 无扩展名、或点在开头（.gitignore）→ 全选
  const end = dot > 0 ? dot : v.length
  el.setSelectionRange(0, end)
}

function onNameFocus(e: FocusEvent) {
  const t = e.target
  if (t instanceof HTMLInputElement) {
    requestAnimationFrame(() => selectFileStem(t))
  }
}

function getNativeInput(): HTMLInputElement | null {
  const exposed = nameInputRef.value as unknown as {
    inputRef?: { value: HTMLInputElement | null } | HTMLInputElement | null
  } | null
  const raw = exposed?.inputRef
  if (!raw) return null
  const el =
    typeof raw === 'object' && raw !== null && 'value' in raw
      ? (raw as { value: HTMLInputElement | null }).value
      : (raw as HTMLInputElement)
  return el instanceof HTMLInputElement ? el : null
}

// 弹窗挂载后聚焦并选中文件名主体
watch(naming, (open) => {
  if (!open) return
  nextTick(() => {
    requestAnimationFrame(() => {
      nameInputRef.value?.focus()
      const el = getNativeInput()
      if (el) selectFileStem(el)
    })
  })
})

// ── 选区文件探测：选中视频/图片文件时显示对应处理入口，跳转目标扩展并带入路径 ──

/** 访达选区视频判断白名单（UI 入口用）。
 * 镜像自 video 扩展 VIDEO_EXTENSIONS，去除 .ts（与 TypeScript 源码歧义）；
 * 真正处理以 video 扩展 ffprobe 为准，此处仅作入口提示。
 * 新增格式时双向同步：extensions/video/logic.ts 的 VIDEO_EXTENSIONS。 */
const VIDEO_EXT_SET = new Set([
  'mp4',
  'mov',
  'mkv',
  'webm',
  'avi',
  'm4v',
  'wmv',
  'flv',
  'mts',
  'm2ts',
  '3gp',
  'mpeg',
  'mpg',
])

/** 访达选区图片判断白名单（UI 入口用）。
 * 镜像自 image 扩展 IMAGE_EXTENSIONS。
 * 新增格式时双向同步：extensions/image/logic.ts 的 IMAGE_EXTENSIONS。 */
const IMAGE_EXT_SET = new Set([
  'png',
  'jpg',
  'jpeg',
  'heic',
  'heif',
  'webp',
  'tiff',
  'tif',
  'bmp',
  'gif',
])

const videoPaths = ref<string[]>([])
const imagePaths = ref<string[]>([])
/** 原始选区（archive 入口用：压缩包 → 解压、其余 → 压缩，由 archive 扩展自行路由） */
const selectionPaths = ref<string[]>([])

/** 浏览模式：应用界面进入（非访达快捷键）——无访达上下文，显示全量操作目录，回车提示仅在访达中生效。 */
const browseMode = ref(false)

/** 上下文可恢复标记：进入 openWith 二级界面（subview）/ 跨扩展跳转（视频 / 图片 / 压缩，
 * 带 from 记忆 Esc 返回目标）期间为 true，重激活 mainView 的 onActivated 消费——保持访达
 * 上下文（选区 / 媒体入口 / open_with 副标题），不重判模式。这些返回路径均无 entryViaShortcut
 * 跟进，不标记会被误判为应用界面进入（浏览模式）清空选区。离开本扩展且不再是返回目标
 * （切扩展不带 from / 退出主界面）时清除，防过期残留误恢复。 */
let contextResumable = false
watch(
  () => appStore.activeExtId,
  (id) => {
    // setActiveExtension 先写 activeExtId 再写 extensionReturnExtId，pre-flush watcher
    // 在两者均落定后运行：jumpTo* 离开时读到返回目标为本扩展 → 保留；其余离开 → 清除
    if (id !== 'finder-ext' && appStore.extensionReturnExtId !== 'finder-ext') {
      contextResumable = false
    }
  },
)

/** 按扩展名过滤（video / image 均收集全部命中供批量处理）。 */
function filterByExt(paths: string[], set: Set<string>): string[] {
  return paths.filter((p) => {
    const ext = p.split('.').pop()?.toLowerCase()
    return !!ext && set.has(ext)
  })
}

/** 探测进行中标志：onActivated 与 reactivateTick 可能同帧触发（快捷键呼出同时切扩展），合并为单次 osascript。 */
let detectInFlight = false

async function detectSelection() {
  if (detectInFlight) return
  detectInFlight = true
  // 先清空：避免 KeepAlive 重激活瞬间显示上次过期选区，探测完成再赋新值
  videoPaths.value = []
  imagePaths.value = []
  selectionPaths.value = []
  try {
    const paths = await invoke<string[]>(CMD.finderSelectedPaths)
    videoPaths.value = filterByExt(paths, VIDEO_EXT_SET)
    imagePaths.value = filterByExt(paths, IMAGE_EXT_SET)
    selectionPaths.value = paths
  } catch {
    // 访达非前台 / 权限缺失 → 不显示入口
    videoPaths.value = []
    imagePaths.value = []
    selectionPaths.value = []
  } finally {
    detectInFlight = false
  }
}

onActivated(() => {
  // 二级界面 / 跨扩展跳转的返回：保持访达上下文（选区 / 媒体入口 / open_with 副标题原样
  // 保留，不重新探测、不切浏览模式）
  if (contextResumable) {
    contextResumable = false
    browseMode.value = false
    return
  }
  // 快捷键进入（makeToggleHandler 同步置位，先于本钩子到达）：访达上下文面板；
  // 应用界面进入（搜索 / 工具列表 / 扩展切换）：浏览模式，不探测选区
  browseMode.value = !entryViaShortcut.value
  entryViaShortcut.value = false
  if (browseMode.value) {
    // 清空上下文（防 KeepAlive 重激活残留上次媒体入口）
    videoPaths.value = []
    imagePaths.value = []
    selectionPaths.value = []
    return
  }
  void detectSelection()
})
// 快捷键呼出（含 KeepAlive 重入）：恒为上下文模式，重新探测选区
watch(reactivateTick, () => {
  browseMode.value = false
  // 无条件消费进入标记：mainView 已激活（窗口隐藏后再呼出，无 onActivated 跟进）与
  // openWith subview 让位停用（若不消费会让标记泄漏——之后退出扩展再从应用界面进入时
  // onActivated 误判为快捷键进入）两路径都在此收口；首挂载路径 watcher 尚不存在
  // （reactivateTick 先于 setActiveExtension 触发），标记留给 onActivated
  entryViaShortcut.value = false
  void detectSelection()
})

function baseName(path: string): string {
  return path.split('/').pop() || path
}

// ── 用 App 打开：目录行承载，回车进入二级界面（OpenWithView）浏览具体 App 列表 ──

/** 进入「用 App 打开」二级界面：聚焦搜索框供即时过滤（MainView 的 subview watch 会
 *  先聚焦滚动容器防失焦藏窗，nextTick 后再聚焦输入框——输入框在窗口内同样不失焦，
 *  且本扩展 query 即过滤参数，二级列表要即打即筛）。 */
function openOpenWith() {
  contextResumable = true
  appStore.openSubview('openWith')
  nextTick(() => {
    ;(document.getElementById('main-search-input') as HTMLInputElement | null)?.focus()
  })
}

/** 跳转视频处理（跨扩展，带入选区路径；多选区全量带入批量处理）。
 * 同页 CustomEvent 同步投递：目标视图首帧渲染即收到路径（经 IPC 往返会晚一拍，
 * 空输入状态的回车是「选择文件」而非「开始处理」，快速连按会误触）。 */
function jumpToVideo() {
  const ps = videoPaths.value
  if (!ps.length) return
  contextResumable = true
  window.dispatchEvent(new CustomEvent('video-pending-input-path', { detail: ps }))
  appStore.setActiveExtension('video', 'finder-ext')
}

/** 跳转压缩/解压（跨扩展，全量带入；全归档 → 解压模式，其余 → 压缩模式，archive 自行路由）。 */
function jumpToArchive() {
  const ps = selectionPaths.value
  if (!ps.length) return
  contextResumable = true
  window.dispatchEvent(new CustomEvent('archive-pending-input-path', { detail: ps }))
  appStore.setActiveExtension('archive', 'finder-ext')
}

/** 跳转图片处理（跨扩展，全量带入；多张由 image 自动进拼接模式）。
 * 同步投递先于跳转首帧（image 的 operations 行按时插入，列表形状稳定）。 */
function jumpToImage() {
  const ps = imagePaths.value
  if (!ps.length) return
  contextResumable = true
  window.dispatchEvent(new CustomEvent('image-pending-input-path', { detail: ps }))
  appStore.setActiveExtension('image', 'finder-ext')
}

/** 媒体入口副标题：单选取文件名，多选取计数。 */
function mediaSubtitle(
  paths: string[],
  countKey: 'finderExt.videoCount' | 'finderExt.imageCount',
): string {
  return paths.length === 1 ? baseName(paths[0]) : t(countKey, { n: paths.length })
}

/** 归档扩展名（镜像自 archive 扩展 ARCHIVE_EXTENSIONS，新增格式双向同步）。
 *  gz/bz2/xz/zst 仅 .tar.* 双扩展算归档——与 archive 侧 Rust detect_by_ext 同语义。 */
const ARCHIVE_EXT_SET = new Set([
  'zip',
  'jar',
  'epub',
  '7z',
  'rar',
  'tar',
  'tgz',
  'tbz2',
  'txz',
  'tzst',
  'iso',
])
const TAR_DOUBLE_EXT = new Set(['gz', 'bz2', 'xz', 'zst'])

function isArchivePath(path: string): boolean {
  const name = path.split('/').pop() ?? ''
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  if (ARCHIVE_EXT_SET.has(ext)) return true
  if (TAR_DOUBLE_EXT.has(ext)) {
    return name
      .slice(0, -(ext.length + 1))
      .toLowerCase()
      .endsWith('.tar')
  }
  return false
}

/** 压缩/解压入口标题与副标题：全归档选区 →「解压」（解压第一个），其余 →「压缩」；
 *  标题与 archive 侧 acceptPaths 路由同源判定。浏览模式无选区，保留通用名。 */
function archiveEntryTitle(): { title: string; subtitle: string | undefined } {
  const ps = selectionPaths.value
  if (!ps.length) return { title: t('finderExt.archiveProcess'), subtitle: undefined }
  const title = ps.every(isArchivePath) ? t('finderExt.extract') : t('finderExt.compress')
  const subtitle =
    ps.length === 1 ? baseName(ps[0]) : t('finderExt.selectionCount', { n: ps.length })
  return { title, subtitle }
}

const allItems = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []
  // 目录行（单一数据源 shortcuts.ts，顺序即正式面板序）；上下文模式：open_with 带
  // 选区副标题、媒体入口仅有选区时出现（带选区副标题）；浏览模式全量显示
  for (const entry of FINDER_CATALOG) {
    let subtitle: string | undefined
    let title = t(entry.titleKey)
    if (!browseMode.value) {
      if (entry.id === 'video_process' && videoPaths.value.length === 0) continue
      if (entry.id === 'image_process' && imagePaths.value.length === 0) continue
      if (entry.id === 'archive_process' && selectionPaths.value.length === 0) continue
      if (entry.id === 'open_with') {
        subtitle =
          selectionPaths.value.length === 1
            ? baseName(selectionPaths.value[0])
            : selectionPaths.value.length > 0
              ? t('finderExt.selectionCount', { n: selectionPaths.value.length })
              : undefined
      } else if (entry.id === 'video_process') {
        subtitle = mediaSubtitle(videoPaths.value, 'finderExt.videoCount')
      } else if (entry.id === 'image_process') {
        subtitle = mediaSubtitle(imagePaths.value, 'finderExt.imageCount')
      } else if (entry.id === 'archive_process') {
        // 全归档选区 →「解压」、其余 →「压缩」（与跳转后的 archive 路由一致），副标题显选区摘要
        const a = archiveEntryTitle()
        title = a.title
        subtitle = a.subtitle
      }
    }
    list.push({
      id: entry.id,
      title,
      subtitle,
      icon: entry.icon,
      type: 'action',
      action: () => {
        // 浏览模式（应用界面进入）：操作依赖访达选区 / 前台，仅提示不执行
        if (browseMode.value) {
          appStore.showStatus(t('finderExt.finderOnly'))
          return
        }
        const id = entry.id
        if (id === 'open_with') {
          openOpenWith()
          return
        }
        if (id === 'new_file') {
          startNaming()
          return
        }
        if (id === 'video_process') {
          jumpToVideo()
          return
        }
        if (id === 'image_process') {
          jumpToImage()
          return
        }
        if (id === 'archive_process') {
          jumpToArchive()
          return
        }
        void runAction(id)
      },
      group: t('finderExt.operations'),
    })
  }
  list.push({
    id: FINDER_SHORTCUT.id,
    title: t('finderExt.shortcut'),
    type: 'shortcut',
    value: shortcutValue.value,
    update: (v) => updateShortcut(String(v)),
    group: t('finderExt.general'),
  })
  return list
})

/** 搜索框过滤（扩展模式 query 即列表过滤参数）：标题 / 副标题包含匹配（快捷键配置行一并过滤）。 */
const filteredItems = computed<SettingItem[]>(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return allItems.value
  return allItems.value.filter(
    (it) =>
      (it.title ?? '').toLowerCase().includes(q) || (it.subtitle ?? '').toLowerCase().includes(q),
  )
})
</script>
