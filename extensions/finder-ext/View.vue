<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" :shortcut-id="FINDER_SHORTCUT.id" />

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
import { computed, nextTick, onActivated, onDeactivated, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore, toastAndHide } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import { t } from '@/runtime/i18n'
import { FINDER_SHORTCUT, FINDER_CATALOG, type FinderAction } from './shortcuts'
import { buildCandidates, fetchApps, type AppEntry } from './apps'
import { config as finderConfig, rememberRecentApp } from './config'
import { entryViaShortcut, reactivateTick } from './index'

const appStore = useAppStore()
const { value: shortcutValue, update: updateShortcut } = useShortcutConfig(
  FINDER_SHORTCUT.id,
  FINDER_SHORTCUT.default,
)

const naming = ref(false)
const fileName = ref('Untitled.txt')
const nameInputRef = ref<InstanceType<typeof BaseInput> | null>(null)

/** Rust 已自行 hide 的动作（勿再抢 hide / toast 时序） */
const HIDE_IN_RUST = new Set<FinderAction>(['toggle_hidden', 'new_file'])

/** @returns 是否成功（供新建文件等决定是否关弹窗） */
async function runAction(action: FinderAction, name?: string, appPath?: string): Promise<boolean> {
  try {
    const msg = await invoke<string>(CMD.finderRunAction, {
      action,
      name: name ?? null,
      appPath: appPath ?? null,
    })
    if (HIDE_IN_RUST.has(action)) {
      return true
    }
    toastAndHide(msg || undefined)
    return true
  } catch (e) {
    appStore.showStatus(String(e ?? t('common.operationFailed')), { duration: 4000, kind: 'error' })
    return false
  }
}

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

/** 浏览模式：应用界面进入（非访达快捷键）——无访达上下文，显示全量操作目录，回车提示仅在访达中生效。 */
const browseMode = ref(false)

/** 视图当前是否处于 KeepAlive 激活态（onActivated / onDeactivated 配对维护）：
 * 快捷键重入时据此判断是否有 onActivated 跟进消费进入标记（窗口隐藏不反激活，仅扩展切换会）。 */
let viewActive = false

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
  openWithCandidates.value = []
  try {
    const paths = await invoke<string[]>(CMD.finderSelectedPaths)
    videoPaths.value = filterByExt(paths, VIDEO_EXT_SET)
    imagePaths.value = filterByExt(paths, IMAGE_EXT_SET)
    void refreshCandidates(paths)
  } catch {
    // 访达非前台 / 权限缺失 → 不显示入口
    videoPaths.value = []
    imagePaths.value = []
  } finally {
    detectInFlight = false
  }
}

onActivated(() => {
  viewActive = true
  // 快捷键进入（makeToggleHandler 同步置位，先于本钩子到达）：访达上下文面板；
  // 应用界面进入（搜索 / 工具列表 / 扩展切换）：浏览模式，不探测选区
  browseMode.value = !entryViaShortcut.value
  entryViaShortcut.value = false
  if (browseMode.value) {
    // 清空上下文（防 KeepAlive 重激活残留上次候选 / 媒体入口）
    videoPaths.value = []
    imagePaths.value = []
    openWithCandidates.value = []
    return
  }
  void detectSelection()
})
onDeactivated(() => {
  viewActive = false
})
// 快捷键呼出（含 KeepAlive 重入）：恒为上下文模式，重新探测选区
watch(reactivateTick, () => {
  browseMode.value = false
  // 已激活态的重入（窗口隐藏后快捷键再呼出，无 onActivated 跟进）：此处消费标记，
  // 防残留 true 让下次应用界面进入（onActivated）误判为快捷键进入
  if (viewActive) entryViaShortcut.value = false
  void detectSelection()
})

function baseName(path: string): string {
  return path.split('/').pop() || path
}

// ── 用 App 打开：面板内联候选（主流做法：MRU 置顶 + 类型推荐补足），无二级界面 ──

const openWithCandidates = ref<AppEntry[]>([])
const allApps = ref<AppEntry[]>([])

/** 候选刷新序号：探测重入（快捷键重呼）时旧轮结果丢弃，防过期候选闪现 */
let candidateSeq = 0

/** 刷新「用 App 打开」候选：LaunchServices 类型推荐（取选区首项，多选按首项类型）+ MRU。 */
async function refreshCandidates(paths: string[]) {
  const seq = ++candidateSeq
  const ls = paths[0]
    ? await invoke<string[]>(CMD.finderOpenWithApps, { path: paths[0] }).catch(() => [])
    : []
  if (seq !== candidateSeq) return
  // 总是经 fetchApps 取已安装列表：缓存命中零开销，失效事件（应用增删/图标就绪）后取新；
  // 失败回退现有列表（join 尽力而为）
  const apps = await fetchApps().catch(() => allApps.value)
  if (seq !== candidateSeq) return
  if (apps.length > 0) allApps.value = apps
  openWithCandidates.value = buildCandidates(apps, ls, finderConfig.recentApps)
}

/** 执行候选行：打开成功记忆 MRU（下次置顶直达）。 */
async function executeOpenWith(app: AppEntry) {
  const ok = await runAction('open_with', undefined, app.path)
  if (ok) rememberRecentApp(app.path)
}

/** 跳转视频处理（跨扩展，带入选区路径；多选区全量带入批量处理）。
 * 同页 CustomEvent 同步投递：目标视图首帧渲染即收到路径（经 IPC 往返会晚一拍，
 * 空输入状态的回车是「选择文件」而非「开始处理」，快速连按会误触）。 */
function jumpToVideo() {
  const ps = videoPaths.value
  if (!ps.length) return
  window.dispatchEvent(new CustomEvent('video-pending-input-path', { detail: ps }))
  appStore.setActiveExtension('video')
}

/** 跳转图片处理（跨扩展，全量带入；多张由 image 自动进拼接模式）。
 * 同步投递先于跳转首帧（image 的 operations 行按时插入，列表形状稳定）。 */
function jumpToImage() {
  const ps = imagePaths.value
  if (!ps.length) return
  window.dispatchEvent(new CustomEvent('image-pending-input-path', { detail: ps }))
  appStore.setActiveExtension('image')
}

/** 媒体入口副标题：单选取文件名，多选取计数。 */
function mediaSubtitle(
  paths: string[],
  countKey: 'finderExt.videoCount' | 'finderExt.imageCount',
): string {
  return paths.length === 1 ? baseName(paths[0]) : t(countKey, { n: paths.length })
}

const allItems = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []
  // 「用 App 打开」候选组置顶（仅上下文模式）：MRU + LaunchServices 类型推荐平铺（回车直达）；
  // 浏览模式无选区上下文，由目录首行「用 App 打开」承载
  if (!browseMode.value) {
    for (const app of openWithCandidates.value) {
      list.push({
        id: `open_with_${app.id}`,
        title: app.name,
        icon: app.icon,
        type: 'action',
        action: () => void executeOpenWith(app),
        group: t('finderExt.openWithGroup'),
      })
    }
  }
  // 目录行（单一数据源 shortcuts.ts，顺序即正式面板序）；上下文模式：open_with 由候选组
  // 承载（目录行省略）、媒体入口仅有选区时出现（带选区副标题）；浏览模式全量显示
  for (const entry of FINDER_CATALOG) {
    let subtitle: string | undefined
    if (!browseMode.value) {
      if (entry.id === 'open_with') continue
      if (entry.id === 'video_process' && videoPaths.value.length === 0) continue
      if (entry.id === 'image_process' && imagePaths.value.length === 0) continue
      if (entry.id === 'video_process') {
        subtitle = mediaSubtitle(videoPaths.value, 'finderExt.videoCount')
      } else if (entry.id === 'image_process') {
        subtitle = mediaSubtitle(imagePaths.value, 'finderExt.imageCount')
      }
    }
    list.push({
      id: entry.id,
      title: t(entry.titleKey),
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
</script>
