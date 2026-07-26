<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" :shortcut-id="FINDER_SHORTCUT.id" />

    <BaseDialog
      v-if="naming"
      title="新建文件"
      variant="form"
      size="sm"
      show-footer
      ok-label="创建"
      :close-on-confirm="false"
      @confirm="confirmNewFile"
      @cancel="cancelNaming"
    >
      <div class="form-field">
        <span class="form-label">文件名</span>
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
import { emit } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore, toastAndHide } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import { FINDER_SHORTCUT, FINDER_ACTIONS, type FinderAction } from './shortcuts'
import { reactivateTick } from './index'

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
async function runAction(action: FinderAction, name?: string): Promise<boolean> {
  try {
    const msg = await invoke<string>(CMD.finderRunAction, {
      action,
      name: name ?? null,
    })
    if (HIDE_IN_RUST.has(action)) {
      return true
    }
    toastAndHide(msg || undefined)
    return true
  } catch (e) {
    appStore.showStatus(String(e ?? '操作失败'), { duration: 4000, kind: 'error' })
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
    appStore.showStatus('文件名不能为空', { kind: 'error' })
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

// ── 选区视频探测：选中视频文件时显示「视频处理」入口，跳转 video 扩展并带入路径 ──

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

const videoPath = ref<string | null>(null)

function pickVideoPath(paths: string[]): string | null {
  for (const p of paths) {
    const ext = p.split('.').pop()?.toLowerCase()
    if (ext && VIDEO_EXT_SET.has(ext)) return p
  }
  return null
}

/** 探测进行中标志：onActivated 与 reactivateTick 可能同帧触发（快捷键呼出同时切扩展），合并为单次 osascript。 */
let detectInFlight = false

async function detectVideo() {
  if (detectInFlight) return
  detectInFlight = true
  // 先清空：避免 KeepAlive 重激活瞬间显示上次过期选区，探测完成再赋新值
  videoPath.value = null
  try {
    const paths = await invoke<string[]>(CMD.finderSelectedPaths)
    videoPath.value = pickVideoPath(paths)
  } catch {
    // 访达非前台 / 权限缺失 → 不显示视频入口
    videoPath.value = null
  } finally {
    detectInFlight = false
  }
}

onActivated(detectVideo)
// 快捷键重入（窗口隐藏后再呼出）：onActivated 不触发，靠 tick 驱动重新探测
watch(reactivateTick, () => void detectVideo())

function baseName(path: string): string {
  return path.split('/').pop() || path
}

const allItems = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []
  // 选中视频时置顶「视频处理」入口（跨扩展跳转，带入路径）
  if (videoPath.value) {
    list.push({
      id: 'video_process',
      title: '视频处理',
      subtitle: baseName(videoPath.value),
      icon: 'i-ri-video-line',
      type: 'action',
      action: () => {
        const p = videoPath.value
        if (!p) return
        void emit('video-pending-input-path', p)
        appStore.setActiveExtension('video')
      },
      group: '操作',
    })
  }
  list.push(
    ...FINDER_ACTIONS.map((a) => ({
      id: a.id,
      title: a.title,
      icon: a.icon,
      type: 'action' as const,
      action: () => {
        if (a.id === 'new_file') {
          startNaming()
          return
        }
        void runAction(a.id)
      },
      group: '操作',
    })),
  )
  list.push({
    id: FINDER_SHORTCUT.id,
    title: '启动快捷键',
    type: 'shortcut',
    value: shortcutValue.value,
    update: (v) => updateShortcut(String(v)),
    group: '通用',
  })
  return list
})
</script>
