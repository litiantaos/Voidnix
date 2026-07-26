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
          下载 FFmpeg
        </BaseButton>
        <div v-else-if="coreLoaded && core.available" flex gap="2">
          <BaseButton :disabled="busy" @click.stop="pickInput">选择</BaseButton>
          <BaseButton v-if="busy" @click.stop="cancelJob">取消</BaseButton>
          <BaseButton
            v-else-if="inputPath"
            variant="primary"
            :disabled="!canRun"
            @click.stop="startJob"
            >开始</BaseButton
          >
        </div>
      </template>
      <template #trailing-outputDir>
        <div flex gap="2">
          <BaseButton v-if="outputDir" @click.stop="resetOutputDir">同目录</BaseButton>
          <BaseButton @click.stop="pickOutputDir">选择</BaseButton>
        </div>
      </template>
    </BaseSettingsList>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore, withSuppressBlur } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import type { SettingItem } from '@/types/settings'
import { config } from './config'
import { pendingInputPath } from './index'
import {
  displayPath,
  fileNameFromPath,
  formatBytes,
  formatMetaLine,
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

const inputPath = ref('')
const meta = ref<VideoMeta | null>(null)

const mode = ref<VideoMode>(config.defaultMode)
const quality = ref<Quality>(config.defaultQuality)
const format = ref<OutputFormat>(config.defaultFormat)
const scale = ref<Scale>(config.defaultScale)
const outputDir = ref(config.outputDir)

const busy = ref(false)
const percent = ref(0)

let unlistenProgress: (() => void) | undefined
let unlistenReady: (() => void) | undefined
let unlistenJob: (() => void) | undefined
/** 本面板发起的 run 进行中：全局 video-job-event 不重复 toast */
let localRun = false
/** 终态 toast 去重（Channel 与 video-job-event 双投递，invoke resolve 先后不确定） */
let lastTerminalToastKey: string | null = null

const downloadText = computed(() => {
  if (downloadTotal.value && downloadTotal.value > 0) {
    const p = Math.min(99, Math.round((downloadReceived.value / downloadTotal.value) * 100))
    return `${p}%`
  }
  if (downloadReceived.value > 0) return formatBytes(downloadReceived.value)
  return '下载中…'
})

const formatsForMode = computed(() => FORMAT_BY_MODE[mode.value])

const canRun = computed(
  () =>
    coreLoaded.value &&
    core.value.available &&
    !!inputPath.value &&
    !busy.value &&
    !isDownloading.value,
)

const progressText = computed(() => {
  if (percent.value <= 0) return ''
  return `${Math.round(percent.value)}%`
})

/** 合并行副标题：未选文件才显示核心版本；已选仅元数据；进行中仅进度 */
const sourceSubtitle = computed(() => {
  if (isDownloading.value) return '正在下载核心…'
  if (!coreLoaded.value) return '核心版本：FFmpeg —'
  if (!core.value.available) return '功能依赖 FFmpeg 核心，请先下载'
  if (busy.value) return `进度 ${progressText.value || '…'}`
  if (!inputPath.value) return `核心版本：FFmpeg ${core.value.version || '—'}`
  if (meta.value) return formatMetaLine(meta.value)
  return displayPath(inputPath.value)
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
      coreLoaded.value && core.value.available && inputPath.value
        ? fileNameFromPath(inputPath.value)
        : '输入视频',
    subtitle: sourceSubtitle.value,
    type: 'custom',
    group: '文件',
  })

  list.push({
    id: 'mode',
    title: '模式',
    type: 'select',
    value: mode.value,
    options: [
      { label: '压缩', value: 'compress' },
      { label: '格式转换', value: 'convert' },
      { label: '提取音频', value: 'extract-audio' },
    ],
    update: (v) => {
      mode.value = v as VideoMode
      config.defaultMode = mode.value
      ensureFormatForMode(mode.value)
    },
    group: '参数',
  })

  // ── 按模式参数 ──
  if (mode.value === 'compress') {
    // 压缩：质量 + 分辨率 + 容器（容器次要）
    list.push(
      {
        id: 'quality',
        title: '质量',
        type: 'select',
        value: quality.value,
        options: [
          { label: '高质量', value: 'high' },
          { label: '均衡', value: 'balanced' },
          { label: '体积优先', value: 'small' },
        ],
        update: (v) => {
          quality.value = v as Quality
          config.defaultQuality = quality.value
        },
        group: '参数',
      },
      {
        id: 'scale',
        title: '分辨率',
        type: 'select',
        value: scale.value,
        options: [
          { label: '原始', value: 'original' },
          { label: '1080p', value: '1080' },
          { label: '720p', value: '720' },
          { label: '480p', value: '480' },
        ],
        update: (v) => {
          scale.value = v as Scale
          config.defaultScale = scale.value
        },
        group: '参数',
      },
      {
        id: 'format',
        title: '容器',
        type: 'select',
        value: format.value,
        options: formatsForMode.value.map((f) => ({ label: f.toUpperCase(), value: f })),
        update: (v) => {
          format.value = v as OutputFormat
          config.defaultFormat = format.value
        },
        group: '参数',
      },
    )
  } else if (mode.value === 'convert') {
    // 转换：目标格式优先 + 质量 + 分辨率
    list.push(
      {
        id: 'format',
        title: '目标格式',
        type: 'select',
        value: format.value,
        options: formatsForMode.value.map((f) => ({ label: f.toUpperCase(), value: f })),
        update: (v) => {
          format.value = v as OutputFormat
          config.defaultFormat = format.value
        },
        group: '参数',
      },
      {
        id: 'quality',
        title: format.value === 'gif' ? '帧率档' : '质量',
        type: 'select',
        value: quality.value,
        options: [
          { label: '高质量', value: 'high' },
          { label: '均衡', value: 'balanced' },
          { label: '体积优先', value: 'small' },
        ],
        update: (v) => {
          quality.value = v as Quality
          config.defaultQuality = quality.value
        },
        group: '参数',
      },
      {
        id: 'scale',
        title: '分辨率',
        type: 'select',
        value: scale.value,
        options: [
          { label: '原始', value: 'original' },
          { label: '1080p', value: '1080' },
          { label: '720p', value: '720' },
          { label: '480p', value: '480' },
        ],
        update: (v) => {
          scale.value = v as Scale
          config.defaultScale = scale.value
        },
        group: '参数',
      },
    )
  } else {
    // 提取音频：格式 + 音质（无分辨率）
    list.push(
      {
        id: 'format',
        title: '音频格式',
        type: 'select',
        value: format.value,
        options: formatsForMode.value.map((f) => ({ label: f.toUpperCase(), value: f })),
        update: (v) => {
          format.value = v as OutputFormat
          config.defaultFormat = format.value
        },
        group: '参数',
      },
      {
        id: 'quality',
        title: '音质',
        type: 'select',
        value: quality.value,
        options: [
          { label: '高（192k）', value: 'high' },
          { label: '标准（128k）', value: 'balanced' },
          { label: '省流（96k）', value: 'small' },
        ],
        update: (v) => {
          quality.value = v as Quality
          config.defaultQuality = quality.value
        },
        group: '参数',
      },
    )
  }

  list.push({
    id: 'outputDir',
    title: '输出目录',
    subtitle: outputDir.value ? displayPath(outputDir.value) : '与源文件相同',
    type: 'custom',
    group: '输出',
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
    appStore.showStatus('FFmpeg 已就绪')
  } catch (e) {
    appStore.showStatus(`下载失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
  } finally {
    isDownloading.value = false
    await refreshCore()
  }
}

async function pickInput() {
  await withSuppressBlur(async () => {
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: false,
      allowedExtensions: [...VIDEO_EXTENSIONS],
    })
    if (paths[0]) await loadInput(paths[0])
  })
}

/** loadInput 单调序号：并发探测时丢弃过期结果，避免 inputPath 与 meta 错位（错位会让 startJob 拿到错误时长，影响进度估算）。 */
let loadInputSeq = 0

/** 设置输入路径并探测元数据（失败 toast）。pickInput 与跨扩展投递共用。 */
async function loadInput(path: string) {
  const seq = ++loadInputSeq
  inputPath.value = path
  try {
    const probed = await invoke<VideoMeta>(CMD.videoProbe, { path })
    // 过期：探测期间已有更新的路径投递，丢弃以免 meta 与当前 inputPath 错位
    if (seq !== loadInputSeq) return
    meta.value = probed
  } catch (e) {
    if (seq !== loadInputSeq) return
    meta.value = null
    appStore.showStatus(`无法读取视频：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
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
    if (busy.value) {
      void cancelJob()
      return
    }
    if (canRun.value) void startJob()
    else void pickInput()
  }
}

function applyJobEvent(ev: VideoEvent, toast: boolean) {
  switch (ev.type) {
    case 'started':
      busy.value = true
      break
    case 'progress':
      busy.value = true
      percent.value = ev.percent
      break
    case 'done':
      percent.value = 100
      busy.value = false
      localRun = false
      if (toast) {
        const key = `done:${ev.outputPath}`
        if (lastTerminalToastKey !== key) {
          lastTerminalToastKey = key
          appStore.showStatus('处理完成')
        }
      }
      break
    case 'error':
      busy.value = false
      localRun = false
      if (toast) {
        const key = `error:${ev.message}`
        if (lastTerminalToastKey !== key) {
          lastTerminalToastKey = key
          if (ev.message !== '已取消') {
            appStore.showStatus(`失败：${ev.message}`, { duration: 5000, kind: 'error' })
          } else {
            appStore.showStatus('已取消')
          }
        }
      }
      break
  }
}

async function startJob() {
  if (!canRun.value) return
  busy.value = true
  percent.value = 0
  localRun = true
  lastTerminalToastKey = null

  const channel = new Channel<VideoEvent>()
  channel.onmessage = (ev) => applyJobEvent(ev, true)

  // 提取音频时分辨率无意义，固定 original；Rust 端仍接收
  const runScale: Scale = mode.value === 'extract-audio' ? 'original' : scale.value

  try {
    await invoke(CMD.videoRun, {
      request: {
        inputPath: inputPath.value,
        outputDir: outputDir.value || null,
        durationSecs: meta.value?.durationSecs ?? 0,
        params: {
          mode: mode.value,
          format: format.value,
          quality: quality.value,
          scale: runScale,
          preferHardware: config.preferHardware,
        },
      },
      onEvent: channel,
    })
  } catch (e) {
    busy.value = false
    localRun = false
    appStore.showStatus(`启动失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
  }
}

async function cancelJob() {
  try {
    await invoke(CMD.videoCancel)
  } catch (e) {
    console.error(e)
  }
}

async function restoreJobStatus() {
  try {
    const snap = await invoke<JobSnapshot>(CMD.videoJobStatus)
    busy.value = snap.busy
    percent.value = snap.lastPercent
  } catch {
    /* ignore */
  }
}

// 跨扩展进入：finder-ext 等经事件总线投递的待处理路径，写入即加载。
// 时序证明（无需 onMounted 兜底）：emit 走 IPC 往返（macrotask），setActiveExtension
// 同步改 ref 触发 Vue flush（microtask）；microtask 必先于 macrotask 清空，故 View
// 挂载 + watch 注册恒先于 IPC 回调到达，watch 不会漏触发。
watch(pendingInputPath, (path) => {
  if (!path) return
  pendingInputPath.value = ''
  void loadInput(path)
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
    applyJobEvent(e.payload, !localRun)
  })
})

onUnmounted(() => {
  unlistenProgress?.()
  unlistenReady?.()
  unlistenJob?.()
})
</script>
