<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items" @execute="onSettingsExecute">
      <!-- 第一项：核心 / 选文件 / 开始·取消 合一（双按钮） -->
      <template #trailing-source>
        <BaseButton v-if="isDownloading" class="min-w-12 tabular-nums" disabled>{{
          downloadText
        }}</BaseButton>
        <BaseButton
          v-else-if="coreLoaded && !core.available"
          variant="primary"
          @click.stop="ensureCore"
        >
          {{ t('video.downloadFFmpeg') }}
        </BaseButton>
        <div v-else-if="coreLoaded && core.available" flex gap="2">
          <BaseButton :disabled="st.busy" @click.stop="pickInput">{{
            t('video.select')
          }}</BaseButton>
          <BaseButton v-if="st.busy" @click.stop="cancelJob">{{ t('video.cancel') }}</BaseButton>
          <BaseButton
            v-else-if="st.paths.length > 0"
            variant="primary"
            :disabled="!canRun"
            @click.stop="startJob"
            >{{ t('video.start') }}</BaseButton
          >
        </div>
      </template>
      <template #trailing-outputDir>
        <div flex gap="2">
          <BaseButton v-if="outputDir" @click.stop="resetOutputDir">{{
            t('video.sameDir')
          }}</BaseButton>
          <BaseButton @click.stop="pickOutputDir">{{ t('video.select') }}</BaseButton>
        </div>
      </template>
    </BaseSettingsList>
  </div>
</template>

<script lang="ts">
import { reactive } from 'vue'

/** 批量状态跨组件实例存活：窗口隐藏 KeepAlive 卸载后队列继续跑，重开面板时
 *  index/total/done/failed 与 busy/percent 不丢（组件 ref 会随卸载与队列断链）。
 *  类型 import 统一在下方 setup 块（SFC 双 script 共享模块作用域，重复 import 报重复标识符）。 */
const st = reactive({
  /** 已选输入（单元素即单文件语义，批处理统一走队列） */
  paths: [] as string[],
  /** 与 paths 平行的探测结果；null = 未探测 / 探测失败（startJob 时 Rust 端自 probe 兜底） */
  metas: [] as (VideoMeta | null)[],
  busy: false,
  percent: 0,
  /** 队列存活：终态推进以本面板 Channel 事件为准，全局 video-job-event 让路（双投递去重） */
  queueActive: false,
  /** 用户请求取消整批 */
  cancelled: false,
  total: 0,
  index: 0,
  done: 0,
  failed: 0,
})

/** 队列发起时的参数快照（批量中途改 UI 参数不影响后续文件；startJob 时整体覆写） */
const batchSnapshot: {
  outputDir: string | null
  params: {
    mode: VideoMode
    format: OutputFormat
    quality: Quality
    scale: Scale
    preferHardware: boolean
  }
} = {
  outputDir: null,
  params: {
    mode: 'compress',
    format: 'mp4',
    quality: 'balanced',
    scale: 'original',
    preferHardware: true,
  },
}

/** 最近一次失败详情（单文件批次失败 toast） */
let lastErrorMessage = ''
</script>

<script setup lang="ts">
import { computed, onActivated, onMounted, onUnmounted, ref, watch } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore, withSuppressBlur } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { t } from '@/runtime/i18n'
import type { SettingItem } from '@/types/settings'
import { config } from './config'
import { pendingInputPaths } from './index'
import {
  displayPath,
  fileNameFromPath,
  formatBytes,
  formatMetaLine,
  summarizeMetas,
  FORMAT_BY_MODE,
  type OutputFormat,
  type Quality,
  type Scale,
  type VideoMode,
  type CoreStatus,
  type VideoMeta,
  type JobSnapshot,
  type VideoEvent,
  VIDEO_EXTENSIONS,
} from './logic'

const appStore = useAppStore()

/** 未拉取完成前不展示「下载」按钮，避免进入时闪一下 */
const coreLoaded = ref(false)
const core = ref<CoreStatus>({
  available: false,
  source: 'none',
  version: '',
  downloading: false,
})
const isDownloading = ref(false)
const downloadReceived = ref(0)
const downloadTotal = ref<number | null>(null)

const mode = ref<VideoMode>(config.defaultMode)
const quality = ref<Quality>(config.defaultQuality)
const format = ref<OutputFormat>(config.defaultFormat)
const scale = ref<Scale>(config.defaultScale)
const outputDir = ref(config.outputDir)

let unlistenProgress: (() => void) | undefined
let unlistenReady: (() => void) | undefined
let unlistenJob: (() => void) | undefined

const downloadText = computed(() => {
  if (downloadTotal.value && downloadTotal.value > 0) {
    const p = Math.min(99, Math.round((downloadReceived.value / downloadTotal.value) * 100))
    return `${p}%`
  }
  if (downloadReceived.value > 0) return formatBytes(downloadReceived.value)
  return t('video.downloading')
})

const formatsForMode = computed(() => FORMAT_BY_MODE[mode.value])

const canRun = computed(
  () =>
    coreLoaded.value &&
    core.value.available &&
    st.paths.length > 0 &&
    !st.busy &&
    !isDownloading.value,
)

const progressText = computed(() => {
  if (st.percent <= 0) return ''
  return `${Math.round(st.percent)}%`
})

/** 合并行副标题：未选文件才显示核心版本；已选仅文件信息；进行中仅进度 */
const sourceSubtitle = computed(() => {
  if (isDownloading.value) return t('video.downloadingCore')
  if (!coreLoaded.value) return t('video.coreVersionNone')
  if (!core.value.available) return t('video.dependencyHint')
  if (st.busy) {
    if (st.total > 1)
      return t('video.batchProgress', {
        i: st.index + 1,
        n: st.total,
        value: progressText.value || '…',
      })
    return t('video.progress', { value: progressText.value || '…' })
  }
  if (st.paths.length === 0) return t('video.coreVersion', { version: core.value.version || '—' })
  if (st.paths.length === 1) {
    return st.metas[0] ? formatMetaLine(st.metas[0]) : displayPath(st.paths[0])
  }
  return summarizeMetas(st.metas) || '—'
})

function ensureFormatForMode(next: VideoMode) {
  const allowed = FORMAT_BY_MODE[next]
  if (!allowed.includes(format.value)) {
    format.value = allowed[0]
    config.defaultFormat = format.value
  }
}

const items = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []

  // ── 输入视频 ⇌ 核心 ⇌ 开始（右侧双按钮，见 trailing-source）──
  list.push({
    id: 'source',
    title:
      coreLoaded.value && core.value.available && st.paths.length > 0
        ? st.paths.length === 1
          ? fileNameFromPath(st.paths[0])
          : t('video.fileCount', { n: st.paths.length })
        : t('video.inputVideo'),
    subtitle: sourceSubtitle.value,
    type: 'custom',
    group: t('video.group.file'),
  })

  list.push({
    id: 'mode',
    title: t('video.mode'),
    type: 'select',
    value: mode.value,
    options: [
      { label: t('video.mode.compress'), value: 'compress' },
      { label: t('video.mode.convert'), value: 'convert' },
      { label: t('video.mode.extractAudio'), value: 'extract-audio' },
    ],
    update: (v) => {
      mode.value = v as VideoMode
      config.defaultMode = mode.value
      ensureFormatForMode(mode.value)
    },
    group: t('video.group.params'),
  })

  // ── 按模式参数 ──
  if (mode.value === 'compress') {
    // 压缩：质量 + 分辨率 + 容器（容器次要）
    list.push(
      {
        id: 'quality',
        title: t('video.quality'),
        type: 'select',
        value: quality.value,
        options: [
          { label: t('video.quality.high'), value: 'high' },
          { label: t('video.quality.balanced'), value: 'balanced' },
          { label: t('video.quality.small'), value: 'small' },
        ],
        update: (v) => {
          quality.value = v as Quality
          config.defaultQuality = quality.value
        },
        group: t('video.group.params'),
      },
      {
        id: 'scale',
        title: t('video.resolution'),
        type: 'select',
        value: scale.value,
        options: [
          { label: t('video.resolution.original'), value: 'original' },
          { label: '1080p', value: '1080' },
          { label: '720p', value: '720' },
          { label: '480p', value: '480' },
        ],
        update: (v) => {
          scale.value = v as Scale
          config.defaultScale = scale.value
        },
        group: t('video.group.params'),
      },
      {
        id: 'format',
        title: t('video.container'),
        type: 'select',
        value: format.value,
        options: formatsForMode.value.map((f) => ({ label: f.toUpperCase(), value: f })),
        update: (v) => {
          format.value = v as OutputFormat
          config.defaultFormat = format.value
        },
        group: t('video.group.params'),
      },
    )
  } else if (mode.value === 'convert') {
    // 转换：目标格式优先 + 质量 + 分辨率
    list.push(
      {
        id: 'format',
        title: t('video.targetFormat'),
        type: 'select',
        value: format.value,
        options: formatsForMode.value.map((f) => ({ label: f.toUpperCase(), value: f })),
        update: (v) => {
          format.value = v as OutputFormat
          config.defaultFormat = format.value
        },
        group: t('video.group.params'),
      },
      {
        id: 'quality',
        title: format.value === 'gif' ? t('video.frameRateTier') : t('video.quality'),
        type: 'select',
        value: quality.value,
        options: [
          { label: t('video.quality.high'), value: 'high' },
          { label: t('video.quality.balanced'), value: 'balanced' },
          { label: t('video.quality.small'), value: 'small' },
        ],
        update: (v) => {
          quality.value = v as Quality
          config.defaultQuality = quality.value
        },
        group: t('video.group.params'),
      },
      {
        id: 'scale',
        title: t('video.resolution'),
        type: 'select',
        value: scale.value,
        options: [
          { label: t('video.resolution.original'), value: 'original' },
          { label: '1080p', value: '1080' },
          { label: '720p', value: '720' },
          { label: '480p', value: '480' },
        ],
        update: (v) => {
          scale.value = v as Scale
          config.defaultScale = scale.value
        },
        group: t('video.group.params'),
      },
    )
  } else {
    // 提取音频：格式 + 音质（无分辨率）
    list.push(
      {
        id: 'format',
        title: t('video.audioFormat'),
        type: 'select',
        value: format.value,
        options: formatsForMode.value.map((f) => ({ label: f.toUpperCase(), value: f })),
        update: (v) => {
          format.value = v as OutputFormat
          config.defaultFormat = format.value
        },
        group: t('video.group.params'),
      },
      {
        id: 'quality',
        title: t('video.audioQuality'),
        type: 'select',
        value: quality.value,
        options: [
          { label: t('video.audioQuality.high'), value: 'high' },
          { label: t('video.audioQuality.balanced'), value: 'balanced' },
          { label: t('video.audioQuality.small'), value: 'small' },
        ],
        update: (v) => {
          quality.value = v as Quality
          config.defaultQuality = quality.value
        },
        group: t('video.group.params'),
      },
    )
  }

  list.push({
    id: 'outputDir',
    title: t('video.outputDir'),
    subtitle: outputDir.value ? displayPath(outputDir.value) : t('video.sameAsSource'),
    type: 'custom',
    group: t('video.group.output'),
  })

  return list
})

async function refreshCore() {
  try {
    core.value = await invoke<CoreStatus>(CMD.videoCoreStatus)
    if (core.value.downloading) isDownloading.value = true
  } catch (e) {
    console.error(e)
  } finally {
    coreLoaded.value = true
  }
}

async function ensureCore() {
  isDownloading.value = true
  downloadReceived.value = 0
  downloadTotal.value = null
  try {
    core.value = await invoke<CoreStatus>(CMD.videoEnsureCore)
    appStore.showStatus(t('video.coreReady'))
  } catch (e) {
    appStore.showStatus(`${t('video.downloadFailed')}：${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  } finally {
    isDownloading.value = false
    await refreshCore()
  }
}

async function pickInput() {
  await withSuppressBlur(async () => {
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: true,
      allowedExtensions: [...VIDEO_EXTENSIONS],
    })
    if (paths.length > 0) await loadInputs(paths)
  })
}

/** loadInputs 单调序号：并发探测时丢弃过期结果，避免 paths 与 metas 错位（错位会让队列拿到错误时长，影响进度估算）。 */
let loadSeq = 0

/** 清空选择（同时作废进行中的探测循环，防止越界回写已清空的 metas）。 */
function resetSelection() {
  loadSeq++
  st.paths = []
  st.metas = []
}

/** 设置输入列表并逐个探测元数据。单文件失败 toast；多文件静默跳过（汇总行缺省、startJob 时 Rust 自 probe 兜底）。 */
async function loadInputs(paths: string[]) {
  const seq = ++loadSeq
  st.paths = paths
  st.metas = paths.map(() => null)
  for (let i = 0; i < paths.length; i++) {
    try {
      const probed = await invoke<VideoMeta>(CMD.videoProbe, { path: paths[i] })
      // 过期：探测期间已有更新的选择，丢弃以免 metas 与当前 paths 错位
      if (seq !== loadSeq) return
      st.metas[i] = probed
    } catch (e) {
      if (seq !== loadSeq) return
      console.error(`[video] probe 失败: ${paths[i]}`, e)
      if (paths.length === 1) {
        appStore.showStatus(`${t('video.cannotReadVideo')}：${e ?? t('common.unknownError')}`, {
          duration: 4000,
          kind: 'error',
        })
      }
    }
  }
}

async function pickOutputDir() {
  const selected = await withSuppressBlur(() => invoke<string>(CMD.pickDirectory))
  if (selected) {
    outputDir.value = selected
    config.outputDir = selected
  }
}

/** 恢复为与源文件同目录（清空 config.outputDir） */
function resetOutputDir() {
  outputDir.value = ''
  config.outputDir = ''
}

function onSettingsExecute(item: SettingItem) {
  if (item.id === 'outputDir') {
    void pickOutputDir()
    return
  }
  // 第一项回车：下载 / 取消 / 开始 / 选择
  if (item.id === 'source') {
    if (isDownloading.value || !coreLoaded.value) return
    if (!core.value.available) {
      void ensureCore()
      return
    }
    if (st.busy) {
      void cancelJob()
      return
    }
    if (canRun.value) startJob()
    else void pickInput()
  }
}

// ─── 批量队列：逐文件复用单任务 video_run，终态经 Channel 事件推进 ───
// Rust 端保持单任务模型（BUSY 互斥）；队列在前端串行发起，失败跳过继续下一个。

function startJob() {
  if (!canRun.value) return
  // 提取音频时分辨率无意义，固定 original；Rust 端仍接收
  const runScale: Scale = mode.value === 'extract-audio' ? 'original' : scale.value
  batchSnapshot.outputDir = outputDir.value || null
  batchSnapshot.params = {
    mode: mode.value,
    format: format.value,
    quality: quality.value,
    scale: runScale,
    preferHardware: config.preferHardware,
  }
  st.total = st.paths.length
  st.index = 0
  st.done = 0
  st.failed = 0
  st.cancelled = false
  st.queueActive = true
  st.busy = true
  runFile(0)
}

/** 发起第 i 个文件。invoke resolve 不区分成败（错误也走 Ok 返回），终态只认 Channel 事件。 */
function runFile(i: number) {
  st.index = i
  st.percent = 0
  const channel = new Channel<VideoEvent>()
  channel.onmessage = (ev) => onRunEvent(ev, true)
  invoke(CMD.videoRun, {
    request: {
      inputPath: st.paths[i],
      outputDir: batchSnapshot.outputDir,
      durationSecs: st.metas[i]?.durationSecs ?? 0,
      params: batchSnapshot.params,
    },
    onEvent: channel,
  }).catch((e) => {
    // 启动即失败（ensure_bins 等 invoke reject）：按该文件失败收尾
    lastErrorMessage = String(e)
    onFileFailed()
  })
}

/** Channel（队列驱动）与全局 video-job-event（孤儿观察）共用事件处理。 */
function onRunEvent(ev: VideoEvent, fromQueue: boolean) {
  // 队列存活时全局事件让路：同一终态 Channel + 全局双投递，以 Channel 为准驱动队列
  if (!fromQueue && st.queueActive) return
  switch (ev.type) {
    case 'started':
      st.busy = true
      break
    case 'progress':
      st.busy = true
      st.percent = ev.percent
      break
    case 'done':
      if (fromQueue) {
        st.done++
        advanceQueue()
      } else {
        st.busy = false
        appStore.showStatus(t('video.processComplete'))
      }
      break
    case 'error':
      if (fromQueue) {
        if (ev.message === '已取消' || st.cancelled) finishBatch(true)
        else {
          lastErrorMessage = ev.message
          onFileFailed()
        }
      } else {
        st.busy = false
        if (ev.message !== '已取消') {
          appStore.showStatus(`${t('video.failed')}：${ev.message}`, {
            duration: 5000,
            kind: 'error',
          })
        } else {
          appStore.showStatus(t('video.canceled'))
        }
      }
      break
  }
}

function onFileFailed() {
  st.failed++
  console.error(
    `[video] 批量第 ${st.index + 1}/${st.total} 个失败：${st.paths[st.index]}`,
    lastErrorMessage,
  )
  advanceQueue()
}

function advanceQueue() {
  if (st.cancelled) {
    finishBatch(true)
    return
  }
  if (st.index + 1 < st.total) runFile(st.index + 1)
  else finishBatch(false)
}

/** 收束整批：单文件保持原文案，多文件汇总（取消不区分）。 */
function finishBatch(cancelled: boolean) {
  st.queueActive = false
  st.busy = false
  if (cancelled) {
    appStore.showStatus(t('video.canceled'))
    return
  }
  // 全部成功即清空选择回到初始态；失败/取消保留列表便于重试或继续处理剩余
  if (st.failed === 0) resetSelection()
  if (st.total === 1) {
    if (st.failed === 0) appStore.showStatus(t('video.processComplete'))
    else {
      appStore.showStatus(`${t('video.failed')}：${lastErrorMessage || t('common.unknownError')}`, {
        duration: 5000,
        kind: 'error',
      })
    }
    return
  }
  if (st.failed === 0) {
    appStore.showStatus(t('video.batchDone', { n: st.done }))
  } else {
    appStore.showStatus(
      t('video.batchDonePartial', { done: st.done, n: st.total, failed: st.failed }),
      {
        duration: 5000,
        kind: 'error',
      },
    )
  }
}

async function cancelJob() {
  if (st.queueActive) st.cancelled = true
  try {
    await invoke(CMD.videoCancel)
  } catch (e) {
    console.error(e)
  }
}

async function restoreJobStatus() {
  try {
    const snap = await invoke<JobSnapshot>(CMD.videoJobStatus)
    // 队列存活时本模块状态即权威（Channel 持续推送）；仅孤儿任务（队列随页面重载丢失）对齐快照
    if (!st.queueActive) {
      st.busy = snap.busy
      st.percent = snap.lastPercent
    }
  } catch {
    /* ignore */
  }
}

// 跨扩展进入：finder-ext 等经事件总线投递的待处理路径，写入即加载。
// 时序证明（无需 onMounted 兜底）：emit 走 IPC 往返（macrotask），setActiveExtension
// 同步改 ref 触发 Vue flush（microtask）；microtask 必先于 macrotask 清空，故 View
// 挂载 + watch 注册恒先于 IPC 回调到达，watch 不会漏触发。
watch(pendingInputPaths, (paths) => {
  if (!paths.length) return
  pendingInputPaths.value = []
  // 队列进行中不接受新投递（paths 是队列索引基准，中途替换会错位；实际不可达——面板互斥，防御）
  if (st.busy) return
  void loadInputs(paths)
})

// 重新进入扩展即新会话：未在处理中的选择清空重置（含 Escape 切走再进、窗口隐藏重唤起
// 后重挂载——onMounted 后必触发 onActivated）。队列运行中（busy）保留进度上下文。
// 首次挂载时选择本为空，清空无副作用；finder-ext 投递经 IPC（macrotask）必晚于此处。
onActivated(() => {
  if (!st.busy) resetSelection()
})

onMounted(async () => {
  // 先对齐模式合法格式（config 可能跨模式残留）
  ensureFormatForMode(mode.value)
  await refreshCore()
  await restoreJobStatus()
  unlistenProgress = await listen<{ received: number; total: number | null }>(
    'video-core-progress',
    (e) => {
      isDownloading.value = true
      downloadReceived.value = e.payload.received
      downloadTotal.value = e.payload.total
    },
  )
  unlistenReady = await listen('video-core-ready', () => {
    isDownloading.value = false
    refreshCore()
  })
  unlistenJob = await listen<VideoEvent>('video-job-event', (e) => {
    onRunEvent(e.payload, false)
  })
})

onUnmounted(() => {
  unlistenProgress?.()
  unlistenReady?.()
  unlistenJob?.()
})
</script>
