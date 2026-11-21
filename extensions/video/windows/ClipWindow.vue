<template>
  <!-- 剪辑窗口：普通应用窗口（overlay 标题栏），工作台视觉遵循应用设计规范（软面/卡片/主题双轨） -->
  <div class="text-primary bg-canvas flex flex-col h-screen w-screen overflow-hidden">
    <!-- 加载 / 错误 -->
    <div v-if="phase !== 'ready'" class="p-6 flex flex-1 items-center justify-center">
      <BaseEmptyState
        :loading="phase !== 'error'"
        :icon="phase === 'error' ? 'i-ri-error-warning-line' : undefined"
        :title="statusText"
        :description="phase !== 'error' ? playSourceName : undefined"
      >
        <template v-if="phase === 'error'" #action>
          <BaseButton @click="closeWindow">{{ t('video.clip.exit') }}</BaseButton>
        </template>
      </BaseEmptyState>
    </div>

    <template v-else>
      <!-- 顶部工具条卡：文件名 + 元数据 chip + 导出（overlay 标题栏下 32px 红绿灯安全区，
           卡内外空白区 mousedown 拖动窗口） -->
      <header class="px-3 pb-3 pt-8 shrink-0 select-none" @mousedown="onHeaderDrag">
        <div class="p-3 soft-card flex gap-3 items-center">
          <span truncate class="text-sm font-medium max-w-[42%] min-w-0">{{ playSourceName }}</span>
          <div class="flex flex-1 gap-1.5 min-w-0 items-center overflow-hidden">
            <span v-for="part in fileMetaParts" :key="part" class="meta-chip shrink-0">
              {{ part }}
            </span>
          </div>
          <div class="ml-auto flex shrink-0 gap-2 items-center">
            <BaseButton
              icon="i-ri-export-line"
              :disabled="exportDisabled"
              :loading="exporting"
              :title="t('video.clip.export')"
              @click="onExport"
            >
              {{ exporting ? `${Math.round(exportPercent)}%` : t('video.clip.export') }}
            </BaseButton>
          </div>
        </div>
      </header>

      <!-- 主体：监视器 + 时间线段面板。间距两档：模块 12（px-3 外距 / pb-3 行距 / gap-3 卡距 / 卡内 p-3）、
           元素 8（gap-2：按钮组 / 卡片间隙与卡内边距） -->
      <div class="px-3 pb-3 flex flex-1 gap-3 min-h-0">
        <!-- 监视器：媒体井 + 传输控制一体卡 -->
        <section class="soft-card flex flex-1 flex-col min-w-0 overflow-hidden">
          <div class="clip-media flex flex-1 min-h-0 items-center justify-center relative">
            <!-- 双解码槽：live 可见承载当前段，stage 隐藏（opacity，不用 display ——
                 保留解码优先级）预滚下一段，刀口处同任务换层（详见播放引擎节） -->
            <video
              ref="videoARef"
              class="clip-layer"
              :class="{ 'is-live': liveIsA }"
              playsinline
              disablepictureinpicture
              preload="auto"
              @loadedmetadata="onSlotMeta"
              @seeked="onSlotSeeked"
              @error="onSlotError"
            />
            <video
              ref="videoBRef"
              class="clip-layer"
              :class="{ 'is-live': !liveIsA }"
              playsinline
              disablepictureinpicture
              preload="auto"
              @loadedmetadata="onSlotMeta"
              @seeked="onSlotSeeked"
              @error="onSlotError"
            />
            <div
              v-if="previewError"
              class="p-6 flex items-center inset-0 justify-center absolute"
              style="background: var(--color-smoke)"
            >
              <p class="text-sm text-white text-center max-w-md">{{ previewError }}</p>
            </div>
          </div>

          <!-- 传输控制条：监视器内底栏。ghost 24px 图标钮与 mark-chip 等高成一条基线；
               播放键 accent 着色为焦点 -->
          <div class="px-3 border-t border-divider flex shrink-0 gap-2 h-10 items-center">
            <div class="flex shrink-0 gap-1.5 items-center">
              <BaseButton
                variant="ghost"
                icon="i-ri-skip-left-line"
                :title="t('video.clip.prevFrame')"
                class="!h-6 !w-6"
                @click="stepFrames(-1)"
              />
              <BaseButton
                variant="ghost"
                :icon="isPlaying ? 'i-ri-pause-fill' : 'i-ri-play-fill'"
                :title="isPlaying ? t('video.clip.pause') : t('video.clip.play')"
                class="!text-accent !h-6 !w-6"
                @click="togglePlay"
              />
              <BaseButton
                variant="ghost"
                icon="i-ri-skip-right-line"
                :title="t('video.clip.nextFrame')"
                class="!h-6 !w-6"
                @click="stepFrames(1)"
              />
            </div>
            <span class="bg-divider mx-2 shrink-0 h-4 w-px" />
            <span class="text-xs font-medium font-mono tabular-nums">
              {{ formatTimecode(playheadUs, playFps) }}
            </span>
            <span class="text-xs text-muted font-mono tabular-nums"
              >/ {{ formatTimecode(totalUs, playFps) }}</span
            >
            <div class="ml-auto flex shrink-0 gap-2 items-center">
              <BaseButton
                variant="ghost"
                icon="i-ri-pulse-line"
                :disabled="analyzing"
                :loading="analyzing"
                :title="t('video.clip.removeSilence')"
                class="!h-6 !w-6"
                @click="analyzeSilence"
              />
              <span class="mark-chip" :class="inMarkUs != null ? 'mark-set' : ''">
                <span class="mark-key">I</span>{{ markText(inMarkUs) }}
              </span>
              <span class="mark-chip" :class="outMarkUs != null ? 'mark-set' : ''">
                <span class="mark-key">O</span>{{ markText(outMarkUs) }}
              </span>
            </div>
          </div>
        </section>

        <!-- 时间线段面板：段列表即成片（顺序 = 播放序），拖拽排序 -->
        <aside class="soft-card flex shrink-0 flex-col w-72 overflow-hidden">
          <div class="px-3 flex gap-2 h-10 items-center">
            <span class="text-xs text-secondary font-medium">{{ t('video.clip.segments') }}</span>
            <span class="count-chip tabular-nums">{{ segments.length }}</span>
          </div>

          <div v-if="!segments.length" class="flex flex-1 min-h-0 items-center justify-center">
            <BaseEmptyState icon="i-ri-scissors-cut-line" />
          </div>

          <div
            v-else
            class="clip-scroll px-3 py-3 flex flex-1 flex-col gap-2 min-h-0 overflow-y-auto"
          >
            <button
              v-for="(seg, index) in segments"
              :key="seg.id"
              class="seg-card group"
              :class="{
                'is-selected': index === selectedSegment,
                'is-drop-target': index === dropTargetIndex,
              }"
              draggable="true"
              @dragstart="onDragStart($event, index)"
              @dragover.prevent="dropTargetIndex = index"
              @dragleave="dropTargetIndex = -1"
              @drop.prevent="onDrop(index)"
              @dragend="dropTargetIndex = -1"
              @click="onSegmentClick(index)"
            >
              <span class="seg-index tabular-nums">{{ index + 1 }}</span>
              <div class="flex flex-1 flex-col gap-0.5 min-w-0">
                <span class="text-[11px] text-secondary font-mono truncate tabular-nums">{{
                  segmentRangeText(seg)
                }}</span>
                <span class="text-[10px] text-muted truncate">{{
                  sourceNameOf(seg.sourceIndex)
                }}</span>
              </div>
              <span class="text-[11px] text-secondary shrink-0 tabular-nums">{{
                formatDuration(segVirtualUs(seg))
              }}</span>
              <span
                class="speed-chip shrink-0 tabular-nums"
                :class="{ 'speed-set': seg.speed !== 1 }"
                :title="t('video.clip.speed')"
                @click.stop="toggleSpeedMenu(index)"
              >
                {{ formatSpeed(seg.speed) }}
              </span>
              <span
                class="rounded-ctrl text-muted opacity-0 flex-center shrink-0 h-5 w-5 transition-opacity duration-[var(--duration-fast)] hover:text-danger group-hover:opacity-100"
                :title="t('video.clip.deleteSegment')"
                @click.stop="removeSegmentAt(index)"
              >
                <i class="i-ri-close-line text-xs" />
              </span>
              <!-- 倍速预设行：点击段上的倍速 chip 展开 -->
              <div
                v-if="speedMenuFor === index"
                class="mt-1 pt-2 border-t border-divider flex flex-wrap gap-1 w-full"
                @click.stop
              >
                <button
                  v-for="preset in SPEED_PRESETS"
                  :key="preset"
                  class="speed-preset tabular-nums"
                  :class="{ 'is-current': preset === seg.speed }"
                  @click.stop="setSpeed(index, preset)"
                >
                  {{ formatSpeed(preset) }}
                </button>
              </div>
            </button>
          </div>

          <div class="px-3 border-t border-divider flex h-10 items-center justify-between">
            <span class="text-xs text-muted">{{ t('video.clip.total') }}</span>
            <span class="text-sm text-accent font-medium tabular-nums">{{
              formatDuration(totalUs)
            }}</span>
          </div>
        </aside>
      </div>

      <!-- 时间轴（成片虚拟时间线：段块 + 标尺 + 打点 + 播放头） -->
      <section class="px-3 pb-3 shrink-0">
        <div class="p-3 soft-card h-[120px] overflow-hidden">
          <canvas
            ref="timelineRef"
            class="h-full w-full block cursor-pointer"
            @pointerdown="onTimelinePointerDown"
            @pointermove="onTimelineHover"
            @pointerleave="onTimelineLeave"
          />
        </div>
      </section>

      <!-- 状态栏：去停顿审阅 / 导出进度 / 导出结果 / 常态快捷键提示 -->
      <footer
        class="text-xs text-muted px-3 border-t border-divider flex shrink-0 gap-3 h-10 items-center"
      >
        <template v-if="pendingCuts.length">
          <i class="i-ri-pulse-line text-warning shrink-0" />
          <span class="text-warning">{{ silenceSummary }}</span>
          <span class="ml-auto shrink-0">{{ t('video.clip.silenceApply') }}</span>
        </template>
        <template v-else-if="exporting">
          <span shrink="0" class="text-secondary">{{ t('video.clip.exportStage') }}</span>
          <div class="w-40" shrink="0">
            <BaseProgress :value="exportPercent" />
          </div>
          <span class="text-secondary tabular-nums" shrink="0"
            >{{ Math.round(exportPercent) }}%</span
          >
          <span truncate>{{ exportStage }}</span>
          <BaseButton size="inline" class="ml-auto" @click="cancelExport">{{
            t('video.cancel')
          }}</BaseButton>
        </template>
        <template v-else-if="exportResult">
          <i class="i-ri-checkbox-circle-line text-success shrink-0" />
          <span class="text-success shrink-0">{{ t('video.clip.exported') }}</span>
          <span truncate class="text-secondary">{{ exportResultName }}</span>
          <BaseButton size="inline" @click="revealResult">{{ t('video.clip.reveal') }}</BaseButton>
          <span class="ml-auto truncate">{{ t('video.clip.hintKeys') }}</span>
        </template>
        <template v-else>
          <!-- 常态只留快捷键提示：时间码/入出点已在传输条呈现，底栏不重复 -->
          <span class="ml-auto truncate">{{ t('video.clip.hintKeys') }}</span>
        </template>
      </footer>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Channel, convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseProgress from '@/components/ui/BaseProgress.vue'
import { CMD } from '@/commands'
import { showToast } from '@/composables/useToast'
import { t } from '@/runtime/i18n'
import {
  SPEED_PRESETS,
  applySourceCuts,
  clampUs,
  deleteVirtualRange,
  displaySeekUs,
  formatDuration,
  formatTimecode,
  frameDurationUs,
  frameToUs,
  locateVirtual,
  newSegmentId,
  prerollParkUs,
  segSourceUs,
  segStartUs,
  segVirtualUs,
  splitAtVirtual,
  timelineTotalUs,
  usToFrame,
  virtualOffsetToSourceUs,
  type FrameRate,
  type TimelineSegment,
} from './clip/clip-logic'

interface ClipMeta {
  path: string
  durationUs: number
  width: number
  height: number
  displayWidth: number
  displayHeight: number
  fpsNum: number
  fpsDen: number
  videoCodec: string
  codecTag: string
  pixFmt: string
  rotation: number
  hasAudio: boolean
  audioCodec: string
  sampleRate: number
  channels: number
  sizeBytes: number
  container: string
  previewable: boolean
}

interface MediaItem {
  path: string
  url: string
}

interface MediaOpen {
  origin: string
  items: MediaItem[]
}

interface ClipTimeline {
  spritePath: string
  frameWidth: number
  frameHeight: number
  count: number
  intervalUs: number
}

interface LoadedClip {
  meta: ClipMeta
  url: string
  sprite: HTMLImageElement | null
  timeline: ClipTimeline | null
}

const THUMB_COUNT = 40
const THUMB_WIDTH = 160

const phase = ref<'probing' | 'timeline' | 'ready' | 'error'>('probing')
const statusText = ref(t('video.clip.probing'))
const clips = ref<LoadedClip[]>([])
/** 时间线段列表 = 成片真相源（顺序即播放序）。 */
const segments = ref<TimelineSegment[]>([])
const selectedSegment = ref(-1)
/** 播放头（虚拟时间线微秒）。 */
const playheadUs = ref(0)
/** 打点（虚拟时间线微秒，删除区间 [I, O)）。 */
const inMarkUs = ref<number | null>(null)
const outMarkUs = ref<number | null>(null)
/** 当前装载进 live 槽的段序号（-1 = 未装载）。 */
const loadedSegIndex = ref(-1)
const isPlaying = ref(false)
/** 双解码槽：live 承载当前段（可见、驱动播放头），stage 预滚下一段；A/B 轮换。 */
const videoARef = ref<HTMLVideoElement | null>(null)
const videoBRef = ref<HTMLVideoElement | null>(null)
const liveIsA = ref(true)
const liveVideoEl = (): HTMLVideoElement | null =>
  liveIsA.value ? videoARef.value : videoBRef.value
const stageVideoEl = (): HTMLVideoElement | null =>
  liveIsA.value ? videoBRef.value : videoARef.value
const timelineRef = ref<HTMLCanvasElement | null>(null)
const videoLoadError = ref(false)

// ---------------------------------------------------------------------------
// 派生状态
// ---------------------------------------------------------------------------

const totalUs = computed(() => timelineTotalUs(segments.value))
const playLoc = computed(() => locateVirtual(segments.value, playheadUs.value))
const playSegIndex = computed(() => playLoc.value?.index ?? loadedSegIndex.value)
const playSeg = computed(() => segments.value[playSegIndex.value] ?? null)
const playFps = computed<FrameRate>(() => playSeg.value?.fps ?? { num: 30, den: 1 })
const playSource = computed<LoadedClip | null>(() => {
  const seg = playSeg.value
  if (!seg) return clips.value[0] ?? null
  return clips.value[seg.sourceIndex] ?? null
})
const playSourceName = computed(() => {
  const path = playSource.value?.meta.path ?? ''
  return path.split('/').pop() || t('video.clip.title')
})
const exportDisabled = computed(() => phase.value !== 'ready' || segments.value.length === 0)

const previewError = computed(() => {
  if (phase.value !== 'ready') return ''
  if (videoLoadError.value) return t('video.clip.previewFailed')
  const meta = playSource.value?.meta
  if (!meta) return ''
  return meta.previewable
    ? ''
    : t('video.clip.notPreviewable', { codec: meta.codecTag || meta.videoCodec })
})

const fileMetaParts = computed<string[]>(() => {
  const meta = playSource.value?.meta
  if (!meta) return []
  const fps = meta.fpsNum / meta.fpsDen
  const parts = [
    formatDuration(meta.durationUs),
    `${meta.displayWidth}×${meta.displayHeight}`,
    Number.isInteger(fps) ? `${fps}fps` : `${fps.toFixed(2)}fps`,
  ]
  const size = formatBytes(meta.sizeBytes)
  if (size) parts.push(size)
  return parts
})

function formatBytes(bytes: number): string {
  if (bytes <= 0) return ''
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value >= 10 || unit === 0 ? Math.round(value) : value.toFixed(1)}${units[unit]}`
}

function segmentRangeText(seg: TimelineSegment): string {
  const start = formatTimecode(frameToUs(seg.inFrame, seg.fps), seg.fps)
  const end = formatTimecode(frameToUs(seg.outFrame, seg.fps), seg.fps)
  return `${start} → ${end}`
}

function sourceNameOf(index: number): string {
  const path = clips.value[index]?.meta.path ?? ''
  return path.split('/').pop() ?? ''
}

function formatSpeed(speed: number): string {
  return `${speed}×`
}

// ---------------------------------------------------------------------------
// 加载
// ---------------------------------------------------------------------------

function fail(message: string) {
  phase.value = 'error'
  statusText.value = message
}

function countInputs(paths: string[]): string[] {
  return paths.filter((p) => p.trim().length > 0)
}

/** bootstrap 单调序号：挂载 take 与 clip-inputs-changed 事件交叠时丢弃过期会话。 */
let bootstrapSeq = 0

async function bootstrap(paths: string[]) {
  const seq = ++bootstrapSeq
  const inputs = countInputs(paths)
  if (!inputs.length) {
    fail(t('video.clip.openFailed'))
    return
  }
  // 新一批输入 = 新会话：全部状态重置（窗口复用或页面重载时不得残留上一次的状态）
  playheadUs.value = 0
  inMarkUs.value = null
  outMarkUs.value = null
  selectedSegment.value = -1
  exportResult.value = null
  pendingCuts.value = []
  videoLoadError.value = false
  phase.value = 'probing'
  statusText.value = t('video.clip.probing')
  try {
    const metas = await invoke<ClipMeta[]>(CMD.videoClipProbe, { paths: inputs })
    if (seq !== bootstrapSeq) return
    const media = await invoke<MediaOpen>(CMD.videoMediaOpen, { paths: inputs })
    if (seq !== bootstrapSeq) return
    const urlByPath = new Map(media.items.map((item) => [item.path, item.url]))
    clips.value = metas.map((meta) => ({
      meta,
      url: urlByPath.get(meta.path) ?? '',
      sprite: null,
      timeline: null,
    }))
    // 素材自动按顺序上轨：每源一段 [0, 全长]，倍速 1
    segments.value = metas.map((meta, sourceIndex) => {
      const fps: FrameRate = { num: meta.fpsNum, den: meta.fpsDen }
      return {
        id: newSegmentId(),
        sourceIndex,
        inFrame: 0,
        outFrame: usToFrame(meta.durationUs, fps),
        fps,
        speed: 1,
      }
    })
    phase.value = 'timeline'
    statusText.value = t('video.clip.timeline')
    await Promise.all(metas.map((_, index) => loadTimelineFor(index)))
    if (seq !== bootstrapSeq) return
    phase.value = 'ready'
    await nextTick()
    bindVideo()
    resizeCanvas()
    seekVirtual(0)
    // app 是 Accessory（无 Dock 图标），窗口从后台打开后需显式把 WKWebView 设为
    // first responder；WKWebView 首次合成前 setFocus 可能无效，多拍重试（幂等）。
    requestClipFocus()
  } catch (err) {
    if (seq === bootstrapSeq) fail(String(err))
  }
}

async function loadTimelineFor(index: number) {
  const clip = clips.value[index]
  if (!clip || clip.timeline) return
  const meta = clip.meta
  const frameWidth = THUMB_WIDTH
  const aspect = meta.displayWidth > 0 ? meta.displayHeight / meta.displayWidth : 9 / 16
  const frameHeight = Math.max(16, Math.round((frameWidth * aspect) / 2) * 2)
  try {
    const timeline = await invoke<ClipTimeline>(CMD.videoClipTimeline, {
      path: meta.path,
      durationUs: meta.durationUs,
      count: THUMB_COUNT,
      frameWidth,
      frameHeight,
    })
    const image = new Image()
    image.src = convertFileSrc(timeline.spritePath)
    try {
      // decode 在资源被拦/加载挂起时可能永不 settle，加超时兜底：无缩略图也要可用
      await Promise.race([
        image.decode(),
        new Promise((_, reject) => setTimeout(() => reject(new Error('sprite timeout')), 4000)),
      ])
      clip.sprite = image
    } catch (err) {
      console.error('sprite load failed', timeline.spritePath, err)
    }
    clip.timeline = timeline
    scheduleDraw()
  } catch (err) {
    console.error('timeline failed', err)
  }
}

// ---------------------------------------------------------------------------
// 播放引擎：双解码槽轮换（live / stage）
//
// 虚拟时间线（段序拼接 + 段倍速）由两个 <video> 解码槽承载：live 可见、驱动播放
// 头；stage 隐藏（opacity，不用 display——保留解码优先级）、预滚下一段（seek 到
// 入点后 2ms 停驻，首帧已解码在层上）。到刀口时预滚就位 → 同任务换层续播（零
// seek 等待、零定格）；未就位 → 退回「先停表再冷切」保底（时钟停在末帧，出点后
// 的帧物理上无法上屏，代价是一刀口时长的定格）。
// ---------------------------------------------------------------------------

function bindVideo() {
  for (const el of [videoARef.value, videoBRef.value]) if (el) el.muted = false
}

/** live 冷切中的待定 seek（源侧微秒，跨源 loadedmetadata 后落位）。 */
let pendingSeekUs: number | null = null
/** live 槽 seek 结算中（冷切/播放态跳转，已停表，落位后续播）：tick 让位。 */
let liveSeeking = false
/** live 槽当前对应的源位置（µs）：seekVirtual 判「位置未变」免停表重落位。 */
let liveSourceUs = 0
/** live 槽已装载段的 id（编辑后索引会漂移，id 才是稳定身份）。 */
let liveSegId: string | null = null

/** 把段装载进 live 槽（调用方保证已按需停表）。seek 目标恒在目标帧呈现窗口内部
 * （displaySeekUs）——非整微秒帧网格（30000/1001、30/24fps）的帧点经双精度秒换算
 * 会落在真实 PTS 之下 ε，呈现的是前一帧（逐帧步进「不走/跳两帧」与刀口呈现已删
 * 帧的共同根因）。 */
function ensureLoaded(segIndex: number, sourceUs: number) {
  const video = liveVideoEl()
  const seg = segments.value[segIndex]
  if (!video || !seg) return
  liveSourceUs = sourceUs
  liveSegId = seg.id
  const url = clips.value[seg.sourceIndex]?.url ?? ''
  if (url !== '' && video.src.startsWith(url)) {
    // 同源段（含切分产物）：无需重载 src。引擎未就绪时该赋值按「默认起始位置」
    // 延迟生效，任何残留的跨源待定 seek 都已过期
    if (loadedSegIndex.value !== segIndex) {
      loadedSegIndex.value = segIndex
      videoLoadError.value = false
    }
    pendingSeekUs = null
    video.currentTime = displaySeekUs(seg, sourceUs) / 1_000_000
    return
  }
  loadedSegIndex.value = segIndex
  videoLoadError.value = false
  pendingSeekUs = displaySeekUs(seg, sourceUs)
  invalidateFrameGuard() // load() 会静默弃置在飞 rVFC 回调：作废防旧位置误触
  video.src = url
  video.load()
}

/** 结构性编辑后的统一收口：钳制播放头并按新时间线重新装载/续播。 */
function syncAfterEdit(atUs: number) {
  if (!segments.value.length) {
    isPlaying.value = false
    playheadUs.value = 0
    loadedSegIndex.value = -1
    liveSegId = null
    stageSegIndex = -1
    stageReady = false
    stageBusy = false
    for (const el of [videoARef.value, videoBRef.value]) {
      if (!el) continue
      el.pause()
      el.removeAttribute('src')
      el.load()
    }
    invalidateFrameGuard()
    scheduleDraw()
    return
  }
  seekVirtual(atUs, isPlaying.value)
}

let rafId = 0

// ---------------------------------------------------------------------------
// 帧级段边界守卫（requestVideoFrameCallback）：末帧「呈现时」切下一段，比删除帧
// （PTS = 出点）的呈现早整整一个帧槽。rAF 轮询（~16ms 粒度）追不上帧边界——删除
// 帧先呈现、下个 tick 才被切走，就是播放到刀口「闪一下已删画面」的根因。仅播放中
// 生效；每次 tick 幂等续挂，暂停/切源自终止。
// ---------------------------------------------------------------------------

let frameGuardArmed = false
let frameGuardToken = 0

/** 作废在飞 rVFC 回调（load() 静默弃置回调、段切换后旧位置回调不得再触新段）。 */
function invalidateFrameGuard() {
  frameGuardToken += 1
  frameGuardArmed = false
}

function armFrameGuard(video: HTMLVideoElement) {
  if (frameGuardArmed || !('requestVideoFrameCallback' in video)) return
  frameGuardArmed = true
  const token = frameGuardToken
  video.requestVideoFrameCallback((_now, meta) => {
    if (token !== frameGuardToken) return
    frameGuardArmed = false
    if (!isPlaying.value || video.paused || video.ended) return
    const seg = segments.value[loadedSegIndex.value]
    if (!seg) return
    const lastFrameUs = frameToUs(seg.outFrame, seg.fps) - frameDurationUs(seg.fps)
    if (meta.mediaTime * 1_000_000 >= lastFrameUs - 200) {
      crossBoundary() // 末帧已上屏：删除帧的槽位留给下一段
      return
    }
    armFrameGuard(video)
  })
}

function tick() {
  rafId = requestAnimationFrame(tick)
  const video = liveVideoEl()
  if (!video || !isPlaying.value || video.paused || video.ended) return
  if (pendingSeekUs != null || liveSeeking) return // 冷切中，等 seek 落位
  const seg = segments.value[loadedSegIndex.value]
  if (!seg) return
  armFrameGuard(video)
  const cur = video.currentTime * 1_000_000
  const segIn = frameToUs(seg.inFrame, seg.fps)
  const segOut = frameToUs(seg.outFrame, seg.fps)
  if (cur >= segOut - frameDurationUs(seg.fps) / 2) {
    // rVFC 缺席时的兜底：末帧窗口过半即切——宁提前半帧，不放删除帧出来
    crossBoundary()
    return
  }
  const offset = Math.max(0, (cur - segIn) / seg.speed)
  playheadUs.value = Math.min(
    totalUs.value,
    segStartUs(segments.value, loadedSegIndex.value) + offset,
  )
}

// ---------------------------------------------------------------------------
// stage 预滚：下一段提前「seek 到入点停驻、首帧已解码」，刀口换层零等待。
// 预滚失败的后果只是回退冷切（慢而正确），不构成正确性依赖。
// ---------------------------------------------------------------------------

let stageSegIndex = -1
let stageParkUs = 0
/** 预滚就位：seeked 落位且目标未失效（刀口换层的资格凭证）。 */
let stageReady = false
/** 预滚进行中（跨源等 metadata 或 seek 未落位）：防同目标重复装载。 */
let stageBusy = false
/** 跨源预滚的待定 seek（源侧微秒，loadedmetadata 后落位）。 */
let stagePendingUs: number | null = null
let prerollTimer = 0

/** 调度预滚；immediate 用于换层/落位/起播后（越早预滚，下个刀口越可能无缝）。 */
function schedulePreroll(immediate = false) {
  window.clearTimeout(prerollTimer)
  if (immediate) {
    prerollStage()
    return
  }
  // 防抖：擦洗/步进连续触发时只在停顿 150ms 后装载一次，避免 seek 抖动
  prerollTimer = window.setTimeout(prerollStage, 150)
}

function prerollStage() {
  const video = stageVideoEl()
  const next = loadedSegIndex.value + 1
  const seg = segments.value[next]
  const url = seg ? (clips.value[seg.sourceIndex]?.url ?? '') : ''
  if (!video || !seg || !url) {
    stageSegIndex = -1
    stageReady = false
    stageBusy = false
    stagePendingUs = null
    return
  }
  const parkUs = prerollParkUs(seg)
  if (
    stageSegIndex === next &&
    stageParkUs === parkUs &&
    video.src.startsWith(url) &&
    (stageReady || stageBusy)
  ) {
    return // 目标未变（倍速在换层时落位，不构成预滚失效）
  }
  stageSegIndex = next
  stageParkUs = parkUs
  stageReady = false
  stageBusy = true
  if (video.src.startsWith(url)) {
    stagePendingUs = null
    video.currentTime = parkUs / 1_000_000
    return
  }
  stagePendingUs = parkUs
  video.src = url
  video.load()
}

// ---------------------------------------------------------------------------
// 刀口三路径：终点停住 / 预滚换层（无缝）/ 停表冷切（保底）
// ---------------------------------------------------------------------------

/**
 * 到达刀口。两条切换路径都不依赖「播放中 seek 即时生效」——引擎（AVPlayer）异步
 * 接住 seek 之前媒体时钟仍在走，会越过出点把已删帧送上屏，播放中的前跳甚至按就近
 * 关键帧落位（关键点常在被删区间内）。末段则停在成片终点。
 */
function crossBoundary() {
  invalidateFrameGuard() // 丢弃旧位置在飞回调（rAF 兜底与 rVFC 双路径不双触发）
  const next = loadedSegIndex.value + 1
  if (next >= segments.value.length) {
    isPlaying.value = false
    liveVideoEl()?.pause()
    playheadUs.value = totalUs.value
    scheduleDraw()
    return
  }
  if (stageReady && stageSegIndex === next) {
    swapLive(next)
    return
  }
  advanceCold(next)
}

/** 无缝换层：预滚槽同任务接管为 live（换可见性 + 原速率续播），原 live 退役。 */
function swapLive(next: number) {
  const seg = segments.value[next]
  const incoming = stageVideoEl()
  const outgoing = liveVideoEl()
  if (!seg || !incoming || !outgoing) return
  outgoing.pause() // 时钟停在末帧；换层在同一任务内完成，无空窗无越点
  liveIsA.value = !liveIsA.value
  loadedSegIndex.value = next
  liveSourceUs = frameToUs(seg.inFrame, seg.fps)
  liveSegId = seg.id
  // 退役槽成为新 stage：预滚状态整体失效，待 schedulePreroll 重定位
  stageSegIndex = -1
  stageReady = false
  stageBusy = false
  stagePendingUs = null
  videoLoadError.value = false
  playheadUs.value = segStartUs(segments.value, next)
  incoming.playbackRate = seg.speed
  if (isPlaying.value) void incoming.play().catch(() => {})
  schedulePreroll(true)
  scheduleDraw()
}

/** 冷切保底：先停表（时钟即刻停在末帧，出点后的帧物理上无法再上屏），seek 落位
 * （同源 seeked / 跨源 loadedmetadata→seeked）后续播，代价是一刀口时长的定格。 */
function advanceCold(next: number) {
  const seg = segments.value[next]
  const video = liveVideoEl()
  if (!seg || !video) return
  video.pause()
  liveSeeking = true
  ensureLoaded(next, frameToUs(seg.inFrame, seg.fps))
}

// ---------------------------------------------------------------------------
// 槽位事件（角色随换层翻转，按事件时的实时角色分派）
// ---------------------------------------------------------------------------

function onSlotMeta(event: Event) {
  const el = event.target as HTMLVideoElement
  if (el === liveVideoEl()) {
    onLiveMeta(el)
    return
  }
  if (stagePendingUs != null) {
    el.currentTime = stagePendingUs / 1_000_000
    stagePendingUs = null
  }
}

function onSlotSeeked(event: Event) {
  const el = event.target as HTMLVideoElement
  if (el !== liveVideoEl()) {
    // stage 落位即预滚就位；stale seeked（load 弃置的旧 seek）不消费
    if (stagePendingUs == null) {
      stageBusy = false
      stageReady = true
    }
    return
  }
  onLiveSeeked()
}

function onSlotError(event: Event) {
  const el = event.target as HTMLVideoElement
  if (el === liveVideoEl()) {
    if (el.src) videoLoadError.value = true
    return
  }
  // stage 预滚失败只回退冷切，不打扰监看
  stageReady = false
  stageBusy = false
  stagePendingUs = null
}

function onLiveMeta(video: HTMLVideoElement) {
  const seg = segments.value[loadedSegIndex.value]
  if (!seg) return
  video.playbackRate = seg.speed
  if (pendingSeekUs != null) {
    video.currentTime = pendingSeekUs / 1_000_000
    pendingSeekUs = null
    // 不在此直接续播：seek 未落位就 play 会从错误位置出帧，等 seeked 收尾
    return
  }
  if (liveSeeking) finishLiveSeek()
}

/** live 槽 seek 结算：按需续播（擦洗会话中除外，抬手统一续），并尽快预滚下一段。 */
function finishLiveSeek() {
  if (!liveSeeking) return
  liveSeeking = false
  const video = liveVideoEl()
  const seg = segments.value[loadedSegIndex.value]
  if (!video || !seg) return
  video.playbackRate = seg.speed
  if (isPlaying.value && !scrubbing) void video.play().catch(() => {})
  schedulePreroll(!scrubbing)
}

function onLiveSeeked() {
  if (pendingSeekUs != null) return // 跨源 load 尚未就绪（stale seeked 不消费）
  if (liveSeeking) {
    finishLiveSeek()
    return
  }
  // 播放态普通 seek（步进/跳转）的续播点；擦洗进行中不续，抬手统一续
  const video = liveVideoEl()
  if (isPlaying.value && !scrubbing && video && video.paused) void video.play().catch(() => {})
}

function togglePlay() {
  const video = liveVideoEl()
  if (!video || !segments.value.length) return
  if (!playSource.value?.meta.previewable) return
  if (isPlaying.value) {
    isPlaying.value = false
    video.pause()
  } else {
    // 播放到末尾后再按播放：从头开始
    if (playheadUs.value >= totalUs.value - 1) seekVirtual(0)
    isPlaying.value = true
    if (liveSeeking) return // 冷切中：seeked 落位时统一续播
    const seg = segments.value[loadedSegIndex.value]
    video.playbackRate = seg?.speed ?? 1
    void video.play().catch(() => {})
    schedulePreroll(true)
  }
}

/** seek 到虚拟时间线位置（跨段自动换源；keepPlaying 落位后续播）。 */
function seekVirtual(us: number, keepPlaying = false) {
  if (!segments.value.length) {
    playheadUs.value = 0
    return
  }
  const target = clampUs(us, totalUs.value)
  const loc = locateVirtual(segments.value, target)
  if (!loc) return
  const seg = segments.value[loc.index]
  const sourceUs = virtualOffsetToSourceUs(seg, loc.offsetUs)
  playheadUs.value = target
  const wasPlaying = isPlaying.value || keepPlaying
  if (keepPlaying) isPlaying.value = true
  const video = liveVideoEl()
  // 装载内容未变（同段 id + 同源位置，如倍速调整/远处编辑后的收口；编辑会让索引
  // 漂移，id 才是稳定身份）：live 槽本就在正确内容上，无需停表重落位——同值
  // currentTime 赋值依赖引擎必发 seeked，一旦短路即停播
  if (segments.value[loc.index]?.id === liveSegId && Math.abs(sourceUs - liveSourceUs) < 1) {
    schedulePreroll()
    scheduleDraw()
    return
  }
  // 播放中的 seek 先停时钟再落位（与刀口同一纪律）：引擎异步接住 seek 之前时钟
  // 仍在走，会越过刀口把已删帧送上屏；落位后由 onLiveSeeked 续播
  if (video && wasPlaying && !video.paused) video.pause()
  if (wasPlaying) liveSeeking = true
  ensureLoaded(loc.index, sourceUs)
  if (video && pendingSeekUs == null) video.playbackRate = seg.speed
  schedulePreroll() // 防抖：擦洗/步进停顿后预滚新位置的下一段
  scheduleDraw()
}

/** 逐帧步进按当前段换算：虚拟步长 = 源单帧 × 段倍速。 */
function stepFrames(delta: number) {
  const seg = playSeg.value
  if (!seg) return
  const stepUs = frameDurationUs(seg.fps) * seg.speed
  seekVirtual(playheadUs.value + delta * stepUs, isPlaying.value)
}

// ---------------------------------------------------------------------------
// 时间轴指针交互（scrub + 悬停）
// ---------------------------------------------------------------------------

let scrubbing = false
const hoverUs = ref<number | null>(null)

function ratioToVirtual(canvas: HTMLCanvasElement, clientX: number): number {
  const rect = canvas.getBoundingClientRect()
  const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
  return ratio * totalUs.value
}

function onTimelinePointerDown(event: PointerEvent) {
  const canvas = timelineRef.value
  if (!canvas || !segments.value.length) return
  scrubbing = true
  hoverUs.value = null
  seekVirtual(ratioToVirtual(canvas, event.clientX), isPlaying.value)
  const onMove = (move: PointerEvent) => {
    seekVirtual(ratioToVirtual(canvas, move.clientX), isPlaying.value)
  }
  const onUp = () => {
    scrubbing = false
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    // 擦洗期间 seeked 不续播（见 onLiveSeeked）：抬手统一续播 + 重定位预滚
    const video = liveVideoEl()
    if (video && isPlaying.value && video.paused) void video.play().catch(() => {})
    schedulePreroll(true)
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
}

function onTimelineHover(event: PointerEvent) {
  const canvas = timelineRef.value
  if (!canvas || scrubbing || !segments.value.length) return
  hoverUs.value = ratioToVirtual(canvas, event.clientX)
  scheduleDraw()
}

function onTimelineLeave() {
  if (hoverUs.value == null) return
  hoverUs.value = null
  scheduleDraw()
}

// ---------------------------------------------------------------------------
// 时间轴绘制（颜色全部取自主题 token，浅/深色自动适配）
// ---------------------------------------------------------------------------

function cssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}

let drawQueued = false
function scheduleDraw() {
  if (drawQueued) return
  drawQueued = true
  requestAnimationFrame(() => {
    drawQueued = false
    drawTimeline()
  })
}

function resizeCanvas() {
  const canvas = timelineRef.value
  if (!canvas) return
  const dpr = window.devicePixelRatio || 1
  const width = canvas.clientWidth
  const height = canvas.clientHeight
  if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
    canvas.width = Math.round(width * dpr)
    canvas.height = Math.round(height * dpr)
  }
}

/** 段内源时间 → 该段精灵图帧号（无精灵返回 -1）。 */
function spriteFrameOf(clip: LoadedClip, sourceUs: number): number {
  if (!clip.timeline) return -1
  const { count, intervalUs } = clip.timeline
  const index = Math.floor(sourceUs / Math.max(1, intervalUs))
  return Math.min(count - 1, Math.max(0, index))
}

function drawTimeline() {
  const canvas = timelineRef.value
  if (!canvas) return
  resizeCanvas()
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const dpr = window.devicePixelRatio || 1
  const width = canvas.clientWidth
  const height = canvas.clientHeight
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, width, height)

  const accent = cssVar('--color-accent', '#3d82f0')
  const warning = cssVar('--color-warning', '#d9891a')
  const track = cssVar('--color-fill-4', 'rgba(120,120,130,0.18)')
  const muted = cssVar('--color-text-muted', 'rgba(110,120,145,0.72)')
  const surface = cssVar('--color-surface', '#f8f8f9')

  const RULER_H = 15
  const trackTop = RULER_H + 4
  const trackBottom = height - 4
  const trackH = Math.max(28, trackBottom - trackTop)
  const guideTop = RULER_H - 2
  const guideBottom = trackBottom

  // 轨道底（圆角槽）
  ctx.fillStyle = track
  ctx.beginPath()
  ctx.roundRect(0, trackTop, width, trackH, 8)
  ctx.fill()

  const total = totalUs.value
  const usToX = (us: number) => (total > 0 ? (us / total) * width : 0)
  const segs = segments.value

  if (total > 0 && segs.length) {
    ctx.save()
    ctx.beginPath()
    ctx.roundRect(0, trackTop, width, trackH, 8)
    ctx.clip()

    // ── 段块：宽度 ∝ 虚拟时长；缩略图按源时间采样铺块 ──
    let cursor = 0
    segs.forEach((seg, index) => {
      const len = segVirtualUs(seg)
      const x0 = (cursor / total) * width
      const w = (len / total) * width
      cursor += len
      const clip = clips.value[seg.sourceIndex]
      if (clip?.sprite && clip.timeline) {
        const sourceIn = frameToUs(seg.inFrame, seg.fps)
        const sourceLen = segSourceUs(seg)
        const { frameWidth, frameHeight } = clip.timeline
        const cell = Math.max(24, w / 12) // 每块最多 ~12 格，避免巨块逐帧铺
        for (let x = 0; x < w; x += cell) {
          const t = sourceIn + (x / w) * sourceLen
          const frame = spriteFrameOf(clip, t)
          if (frame >= 0) {
            ctx.drawImage(
              clip.sprite!,
              frame * frameWidth,
              0,
              frameWidth,
              frameHeight,
              x0 + x,
              trackTop,
              Math.min(cell + 1, w - x),
              trackH,
            )
          }
        }
      } else {
        ctx.fillStyle = cssVar('--color-fill-5', 'rgba(120,120,130,0.1)')
        ctx.fillRect(x0, trackTop, w, trackH)
      }
      // 源边界分隔线
      if (index > 0) {
        ctx.strokeStyle = cssVar('--color-divider', 'rgba(120,120,160,0.2)')
        ctx.lineWidth = 1
        ctx.beginPath()
        ctx.moveTo(x0 + 0.5, trackTop)
        ctx.lineTo(x0 + 0.5, trackBottom)
        ctx.stroke()
      }
      // 倍速角标（≠1 时）
      if (seg.speed !== 1) {
        ctx.fillStyle = accent
        ctx.font = '600 9px ui-monospace, monospace'
        ctx.fillText(formatSpeed(seg.speed), Math.min(x0 + w - 20, x0 + 3), trackTop + 10)
      }
    })

    // ── 去停顿审阅：待删区间（源级映射到虚拟时间线） ──
    if (pendingCuts.value.length && pendingCutsSourceIndex >= 0) {
      ctx.fillStyle = cssVar('--color-danger-soft', 'rgba(236,106,110,0.2)')
      for (const range of pendingVirtualRanges()) {
        ctx.fillRect(
          usToX(range.startUs),
          trackTop,
          usToX(range.endUs) - usToX(range.startUs),
          trackH,
        )
      }
    }

    // ── 打点区间（warning 浅染） ──
    if (inMarkUs.value != null && outMarkUs.value != null && outMarkUs.value > inMarkUs.value) {
      ctx.fillStyle = cssVar('--color-warning-soft', 'rgba(217,137,26,0.12)')
      ctx.fillRect(
        usToX(inMarkUs.value),
        trackTop,
        Math.max(2, usToX(outMarkUs.value) - usToX(inMarkUs.value)),
        trackH,
      )
    }
    ctx.restore()

    // ── 选中段描边 ──
    if (selectedSegment.value >= 0 && selectedSegment.value < segs.length) {
      const x0 = usToX(segStartUs(segs, selectedSegment.value))
      const x1 = usToX(
        segStartUs(segs, selectedSegment.value) + segVirtualUs(segs[selectedSegment.value]),
      )
      ctx.strokeStyle = accent
      ctx.lineWidth = 1.5
      ctx.strokeRect(x0 + 0.75, trackTop + 0.75, Math.max(2, x1 - x0) - 1.5, trackH - 1.5)
    }

    // ── I/O 标记：竖线 + 标尺位旗标字母 ──
    const drawMarker = (us: number, label: string) => {
      const x = usToX(us)
      ctx.strokeStyle = warning
      ctx.lineWidth = 1.5
      ctx.beginPath()
      ctx.moveTo(x, guideTop)
      ctx.lineTo(x, trackBottom)
      ctx.stroke()
      ctx.fillStyle = warning
      ctx.font = '700 9px ui-monospace, monospace'
      ctx.fillText(label, Math.min(width - 8, Math.max(2, x + 3)), RULER_H - 3)
    }
    if (inMarkUs.value != null) drawMarker(inMarkUs.value, 'I')
    if (outMarkUs.value != null) drawMarker(outMarkUs.value, 'O')

    // ── 播放头：accent 竖线 + 顶部手柄 ──
    const headX = Math.min(width, Math.max(0, usToX(playheadUs.value)))
    ctx.strokeStyle = accent
    ctx.lineWidth = 1.5
    ctx.beginPath()
    ctx.moveTo(headX, guideTop)
    ctx.lineTo(headX, guideBottom)
    ctx.stroke()
    ctx.fillStyle = accent
    ctx.beginPath()
    ctx.roundRect(headX - 4, 1, 8, 11, 3)
    ctx.fill()

    // ── 标尺刻度 + 时间标签 ──
    const seconds = total / 1_000_000
    const step = seconds <= 15 ? 1 : seconds <= 60 ? 5 : seconds <= 300 ? 30 : 60
    ctx.font = '9px ui-monospace, monospace'
    for (let s = 0; s <= seconds; s += step) {
      const x = (s / seconds) * width
      if (x > width - 26) break
      ctx.fillStyle = muted
      ctx.fillText(formatClock(s), x + 2, 9)
      ctx.strokeStyle = cssVar('--color-divider', 'rgba(120,120,160,0.1)')
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.moveTo(x + 0.5, 10)
      ctx.lineTo(x + 0.5, 14)
      ctx.stroke()
    }

    // ── 悬停指示：muted 虚线 + 时间码气泡 ──
    if (hoverUs.value != null && !scrubbing) {
      const x = Math.min(width, Math.max(0, usToX(hoverUs.value)))
      ctx.strokeStyle = cssVar('--color-text-muted', muted)
      ctx.lineWidth = 1
      ctx.setLineDash([3, 3])
      ctx.beginPath()
      ctx.moveTo(x + 0.5, guideTop)
      ctx.lineTo(x + 0.5, guideBottom)
      ctx.stroke()
      ctx.setLineDash([])
      const label = formatTimecode(hoverUs.value, playFps.value)
      ctx.font = '9px ui-monospace, monospace'
      const tw = ctx.measureText(label).width
      const bx = Math.min(width - tw - 12, Math.max(2, x - tw / 2 - 4))
      // 气泡贴轨道底部浮在缩略图上
      const by = trackBottom - 18
      ctx.fillStyle = cssVar('--color-fill-18', 'rgba(120,120,160,0.3)')
      ctx.beginPath()
      ctx.roundRect(bx, by, tw + 8, 14, 4)
      ctx.fill()
      ctx.fillStyle = surface
      ctx.fillText(label, bx + 4, by + 10)
    }
  }
}

function formatClock(totalSeconds: number): string {
  const m = Math.floor(totalSeconds / 60)
  const s = Math.floor(totalSeconds % 60)
  return `${m}:${s < 10 ? '0' : ''}${s}`
}

// ---------------------------------------------------------------------------
// 打点 / 删除 / 切分 / 排序 / 倍速（三大核心操作）
// ---------------------------------------------------------------------------

function setIn() {
  inMarkUs.value = playheadUs.value
  if (outMarkUs.value != null && outMarkUs.value <= inMarkUs.value) outMarkUs.value = null
  scheduleDraw()
}

function setOut() {
  outMarkUs.value = playheadUs.value
  if (inMarkUs.value != null && outMarkUs.value <= inMarkUs.value) inMarkUs.value = null
  scheduleDraw()
}

function clearMarks() {
  inMarkUs.value = null
  outMarkUs.value = null
  scheduleDraw()
}

function markText(us: number | null): string {
  return us == null ? t('video.clip.noOut') : formatTimecode(us, playFps.value)
}

/** Enter：删除打点区间（可跨段/切半段，余下涟漪前移），播放头停在删除起点。 */
function deleteMarkedRange() {
  const start = inMarkUs.value
  const end = outMarkUs.value
  if (start == null || end == null || end <= start) {
    showToast(t('video.clip.needRange'), { kind: 'error' })
    return
  }
  segments.value = deleteVirtualRange(segments.value, start, end)
  selectedSegment.value = -1
  speedMenuFor.value = null
  inMarkUs.value = null
  outMarkUs.value = null
  syncAfterEdit(start)
}

function removeSegmentAt(index: number) {
  if (index < 0 || index >= segments.value.length) return
  const at = segStartUs(segments.value, index)
  segments.value.splice(index, 1)
  if (selectedSegment.value === index) selectedSegment.value = -1
  else if (selectedSegment.value > index) selectedSegment.value -= 1
  speedMenuFor.value = null
  syncAfterEdit(at)
}

function removeSelectedSegment() {
  if (selectedSegment.value < 0) return
  removeSegmentAt(selectedSegment.value)
}

/** S：在播放头处切一刀（所在段一分为二）。 */
function splitAtPlayhead() {
  const before = segments.value.length
  segments.value = splitAtVirtual(segments.value, playheadUs.value)
  if (segments.value.length === before) return
  speedMenuFor.value = null
  syncAfterEdit(playheadUs.value)
}

function onSegmentClick(index: number) {
  selectedSegment.value = index
  seekVirtual(segStartUs(segments.value, index))
}

// 拖拽排序（故事板内）
let dragIndex = -1
const dropTargetIndex = ref(-1)

function onDragStart(event: DragEvent, index: number) {
  dragIndex = index
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', String(index))
  }
}

function onDrop(index: number) {
  const target = dropTargetIndex.value
  dropTargetIndex.value = -1
  if (dragIndex < 0 || dragIndex === index || dragIndex === target) {
    dragIndex = -1
    return
  }
  moveSegment(dragIndex, target >= 0 ? target : index)
  dragIndex = -1
}

/** 移动段到新位置（涟漪：列表重排天然承载）。 */
function moveSegment(from: number, to: number) {
  if (from === to || from < 0 || to < 0 || from >= segments.value.length) return
  const next = [...segments.value]
  const [item] = next.splice(from, 1)
  next.splice(to, 0, item)
  segments.value = next
  selectedSegment.value = to
  speedMenuFor.value = null
  syncAfterEdit(playheadUs.value)
}

// 倍速：段卡 chip 展开/收起预设
const speedMenuFor = ref<number | null>(null)

function toggleSpeedMenu(index: number) {
  selectedSegment.value = index
  speedMenuFor.value = speedMenuFor.value === index ? null : index
}

function setSpeed(segIndex: number, speed: number) {
  const seg = segments.value[segIndex]
  if (!seg) return
  segments.value = segments.value.map((s, i) => (i === segIndex ? { ...s, speed } : s))
  speedMenuFor.value = null
  // 总长变化：钳制播放头并同步已装载段的播放速率
  syncAfterEdit(playheadUs.value)
  const video = liveVideoEl()
  if (video && pendingSeekUs == null && loadedSegIndex.value === segIndex) {
    video.playbackRate = speed
  }
}

/** `-`/`=`：选中段（无选中则播放头段）在倍速预设档间循环。 */
function cycleSpeed(direction: 1 | -1) {
  const index = selectedSegment.value >= 0 ? selectedSegment.value : playSegIndex.value
  const seg = segments.value[index]
  if (!seg) return
  const current = SPEED_PRESETS.indexOf(seg.speed as (typeof SPEED_PRESETS)[number])
  const next = Math.min(
    SPEED_PRESETS.length - 1,
    Math.max(0, (current === -1 ? SPEED_PRESETS.indexOf(1) : current) + direction),
  )
  selectedSegment.value = index
  setSpeed(index, SPEED_PRESETS[next])
}

// ---------------------------------------------------------------------------
// 一键去停顿：分析（源级）→ 审阅（高亮映射到虚拟时间线）→ 应用为区间删除
// ---------------------------------------------------------------------------

/** 气口保留：静音段两端各留 150ms，避免切掉呼吸/齿音。 */
const GAP_US = 150_000
/** 短于此值的删减不值得切（叠加气口后收益太小）。 */
const MIN_CUT_US = 120_000

const analyzing = ref(false)
/** 审阅中的废段（源坐标系微秒）与所属源。 */
const pendingCuts = ref<Array<{ startUs: number; endUs: number }>>([])
let pendingCutsSourceIndex = -1

const silenceSummary = computed(() => {
  if (!pendingCuts.value.length) return ''
  const total = pendingCuts.value.reduce((sum, cut) => sum + (cut.endUs - cut.startUs), 0)
  return t('video.clip.silenceSummary', {
    n: pendingCuts.value.length,
    duration: formatDuration(total),
    gap: GAP_US / 1000,
  })
})

/** 源级废段 → 虚拟时间线区间（映射进该源的每一段实例）。 */
function pendingVirtualRanges(): Array<{ startUs: number; endUs: number }> {
  const out: Array<{ startUs: number; endUs: number }> = []
  segments.value.forEach((seg) => {
    if (seg.sourceIndex !== pendingCutsSourceIndex) return
    const segIn = frameToUs(seg.inFrame, seg.fps)
    const segOut = frameToUs(seg.outFrame, seg.fps)
    const base = segStartUs(segments.value, segments.value.indexOf(seg))
    for (const cut of pendingCuts.value) {
      const start = Math.max(cut.startUs, segIn)
      const end = Math.min(cut.endUs, segOut)
      if (end <= start) continue
      out.push({
        startUs: base + (start - segIn) / seg.speed,
        endUs: base + (end - segIn) / seg.speed,
      })
    }
  })
  return out
}

async function analyzeSilence() {
  const source = playSource.value
  const seg = playSeg.value
  if (!source || !seg || analyzing.value) return
  if (!source.meta.hasAudio) {
    void showToast(t('video.clip.noAudio'), { kind: 'error' })
    return
  }
  analyzing.value = true
  pendingCuts.value = []
  try {
    const ranges = await invoke<Array<{ startUs: number; endUs: number }>>(CMD.videoClipAnalyze, {
      path: source.meta.path,
    })
    const cuts = ranges
      .map((range) => ({
        startUs: range.startUs + GAP_US,
        endUs: range.endUs - GAP_US,
      }))
      .filter((cut) => cut.endUs - cut.startUs >= MIN_CUT_US)
    if (!cuts.length) {
      void showToast(t('video.clip.noSilence'))
      return
    }
    pendingCutsSourceIndex = seg.sourceIndex
    pendingCuts.value = cuts
    scheduleDraw()
  } catch (err) {
    void showToast(`${t('video.clip.silenceFailed')}：${err}`, { kind: 'error', duration: 4000 })
  } finally {
    analyzing.value = false
  }
}

function applySilence() {
  if (!pendingCuts.value.length || pendingCutsSourceIndex < 0) return
  const cuts = pendingCuts.value
  const n = cuts.length
  segments.value = applySourceCuts(segments.value, pendingCutsSourceIndex, cuts)
  pendingCuts.value = []
  pendingCutsSourceIndex = -1
  selectedSegment.value = -1
  speedMenuFor.value = null
  void showToast(t('video.clip.silenceApplied', { n }))
  syncAfterEdit(playheadUs.value)
}

function cancelSilence() {
  pendingCuts.value = []
  pendingCutsSourceIndex = -1
  scheduleDraw()
}

// ---------------------------------------------------------------------------
// 导出
// ---------------------------------------------------------------------------

interface ExportResult {
  outputPath: string
  sizeBytes: number
  durationUs: number
}

type ClipExportEvent =
  | { type: 'started'; totalUs: number; segments: number }
  | { type: 'progress'; percent: number; stage: string }
  | { type: 'done'; outputPath: string; sizeBytes: number; durationUs: number }
  | { type: 'error'; message: string }

const exporting = ref(false)
const exportPercent = ref(0)
const exportStage = ref('')
const exportResult = ref<ExportResult | null>(null)
/** 用户已请求取消：错误回报按「已取消导出」呈现（取消竞态下仍可能 done，done 时复位）。 */
let cancelRequested = false

const exportResultName = computed(() => {
  const path = exportResult.value?.outputPath ?? ''
  return path.split('/').pop() ?? path
})

async function onExport() {
  if (exporting.value || !segments.value.length) return
  exportResult.value = null
  exporting.value = true
  exportPercent.value = 0
  exportStage.value = ''
  cancelRequested = false
  const channel = new Channel<ClipExportEvent>()
  channel.onmessage = (event) => {
    switch (event.type) {
      case 'started':
        exportStage.value = ''
        break
      case 'progress':
        exportPercent.value = event.percent
        exportStage.value = event.stage
        break
      case 'done':
        exporting.value = false
        exportPercent.value = 100
        cancelRequested = false
        exportResult.value = {
          outputPath: event.outputPath,
          sizeBytes: event.sizeBytes,
          durationUs: event.durationUs,
        }
        void showToast(t('video.clip.exported'))
        break
      case 'error':
        exporting.value = false
        exportPercent.value = 0
        if (cancelRequested) {
          cancelRequested = false
          void showToast(t('video.clip.exportCanceled'))
        } else {
          void showToast(`${t('video.clip.exportFailed')}：${event.message}`, {
            kind: 'error',
            duration: 4000,
          })
        }
        break
      default:
        break
    }
  }
  // 导出计划 = 时间线段原样（含倍速），交给重编码 + 拼接管线
  const request = {
    segments: segments.value.map((seg) => ({
      path: clips.value[seg.sourceIndex]?.meta.path ?? '',
      inUs: frameToUs(seg.inFrame, seg.fps),
      outUs: frameToUs(seg.outFrame, seg.fps),
      speed: seg.speed,
    })),
    outputPath: null,
    mode: 'precise',
    targetHeight: 0,
  }
  // 运行期错误只经 Channel 回报（Rust 侧 Error 事件后返回 Ok），invoke reject 仅剩
  // 启动前置错误（空片段 / 任务占用）
  await invoke(CMD.videoClipExport, { request, onEvent: channel }).catch((err: unknown) => {
    exporting.value = false
    exportPercent.value = 0
    void showToast(`${t('video.clip.exportFailed')}：${err}`, { kind: 'error', duration: 4000 })
  })
}

async function cancelExport() {
  cancelRequested = true
  await invoke(CMD.videoClipCancel).catch(() => {})
}

async function revealResult() {
  const path = exportResult.value?.outputPath
  if (!path) return
  await invoke(CMD.revealInFinder, { path }).catch(() => {})
}

// ---------------------------------------------------------------------------
// 窗口 / 键盘
// ---------------------------------------------------------------------------

/** 头部空白区拖动窗口（overlay 无原生标题栏）。按钮等交互元素不触发。 */
function onHeaderDrag(event: MouseEvent) {
  if (event.button !== 0) return
  if ((event.target as HTMLElement).closest('button')) return
  void getCurrentWindow()
    .startDragging()
    .catch(() => {})
}

function closeWindow() {
  // 关闭走 Rust 命令：销毁前清理媒体服务/临时素材（窗口 Destroyed 钩子）。
  void invoke(CMD.videoClipClose).catch(() => {})
}

function onKeyDown(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key === 'w') {
    event.preventDefault()
    closeWindow()
    return
  }
  if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
    event.preventDefault()
    void onExport()
    return
  }
  // 去停顿审阅态：Enter 应用 / Esc 取消（优先于常规键位）
  if (pendingCuts.value.length) {
    if (event.key === 'Enter') {
      event.preventDefault()
      applySilence()
      return
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      cancelSilence()
      return
    }
  }
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'd') {
    event.preventDefault()
    void analyzeSilence()
    return
  }
  // ⌘↑/⌘↓：移动选中段（与拖拽同效）
  if ((event.metaKey || event.ctrlKey) && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
    event.preventDefault()
    if (selectedSegment.value >= 0) {
      const delta = event.key === 'ArrowUp' ? -1 : 1
      moveSegment(selectedSegment.value, selectedSegment.value + delta)
    }
    return
  }
  switch (event.key) {
    case ' ':
      event.preventDefault()
      togglePlay()
      break
    case 'l':
    case 'L':
      togglePlay()
      break
    case 'k':
    case 'K':
      isPlaying.value = false
      liveVideoEl()?.pause()
      break
    case 'j':
    case 'J':
      liveVideoEl()?.pause()
      isPlaying.value = false
      seekVirtual(playheadUs.value - 500_000)
      break
    case 'i':
    case 'I':
      setIn()
      break
    case 'o':
    case 'O':
      setOut()
      break
    case 'x':
    case 'X':
      clearMarks()
      break
    case 'Enter':
      event.preventDefault()
      deleteMarkedRange()
      break
    case 's':
    case 'S':
      splitAtPlayhead()
      break
    case '=':
    case '+':
      event.preventDefault()
      cycleSpeed(1)
      break
    case '-':
    case '_':
      event.preventDefault()
      cycleSpeed(-1)
      break
    case 'Backspace':
    case 'Delete':
      event.preventDefault()
      removeSelectedSegment()
      break
    case 'ArrowLeft':
      event.preventDefault()
      if (event.shiftKey) seekVirtual(playheadUs.value - 1_000_000, isPlaying.value)
      else if (event.metaKey) seekVirtual(playheadUs.value - 5_000_000, isPlaying.value)
      else stepFrames(-1)
      break
    case 'ArrowRight':
      event.preventDefault()
      if (event.shiftKey) seekVirtual(playheadUs.value + 1_000_000, isPlaying.value)
      else if (event.metaKey) seekVirtual(playheadUs.value + 5_000_000, isPlaying.value)
      else stepFrames(1)
      break
    case 'Tab':
      event.preventDefault()
      // Tab 循环选中段（Shift 反向）：键盘可达的选中入口（排序/倍速/删除的前提）
      if (segments.value.length) {
        const cur = selectedSegment.value
        const delta = event.shiftKey ? -1 : 1
        selectedSegment.value = (cur + delta + segments.value.length) % segments.value.length
      }
      break
    case 'Escape':
      event.preventDefault()
      if (speedMenuFor.value != null) {
        speedMenuFor.value = null
        return
      }
      closeWindow()
      break
    default:
      break
  }
}

// ---------------------------------------------------------------------------
// 生命周期
// ---------------------------------------------------------------------------

let unlistenInputs: (() => void) | null = null

function requestClipFocus(attempt = 0) {
  void getCurrentWebview()
    .setFocus()
    .catch(() => {})
    .then(() => {
      if (attempt < 3) {
        setTimeout(() => requestClipFocus(attempt + 1), 150 * (attempt + 1))
      }
    })
}

onMounted(async () => {
  requestAnimationFrame(tick)
  // 先注册输入事件再 take/bootstrap：挂载 bootstrap 期间用户再次投喂素材时事件不丢
  //（take 只在挂载时消费一次，事件是复用窗口唯一的后续输入通道）
  unlistenInputs = await listen<string[]>('clip-inputs-changed', (event) => {
    void bootstrap(event.payload)
  }).catch(() => null)
  const paths = await invoke<string[]>(CMD.videoClipTakeInputs).catch(() => [] as string[])
  await bootstrap(paths)
  window.addEventListener('keydown', onKeyDown)
  window.addEventListener('resize', scheduleDraw)
})

onBeforeUnmount(() => {
  cancelAnimationFrame(rafId)
  window.clearTimeout(prerollTimer)
  if (exporting.value) void invoke(CMD.videoClipCancel).catch(() => {})
  for (const el of [videoARef.value, videoBRef.value]) el?.pause()
  window.removeEventListener('keydown', onKeyDown)
  window.removeEventListener('resize', scheduleDraw)
  unlistenInputs?.()
  void invoke(CMD.videoMediaClose).catch(() => {})
})

watch(playheadUs, scheduleDraw)
watch([inMarkUs, outMarkUs, selectedSegment], scheduleDraw)
</script>

<style scoped>
/* 剪辑工作台仅保留应用规范之外的工作台特有件：媒体井 / 打点 chip / 段卡 / 倍速 chip /
   细滚动条。颜色/材质/圆角/阴影/动效全部走主题 token（theme.css / uno shortcuts），
   浅色与深色由 data-theme 自动切换。 */
.clip-media {
  /* 媒体井近黑底：视频监视器的行业惯例内容画布，主题无关（内容底材例外，同 mask-smoke） */
  background: #0b0c0e;
}

/* 双解码槽叠层：stage 用 opacity 隐藏（不用 display:none——保留解码优先级，
   预滚帧必须真实解码在层上，刀口换层才零等待） */
.clip-layer {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
  opacity: 0;
}

.clip-layer.is-live {
  opacity: 1;
}

.meta-chip {
  display: inline-flex;
  align-items: center;
  height: 24px;
  padding: 0 8px;
  border-radius: var(--radius-ctrl);
  background: var(--color-fill-4);
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.mark-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 8px;
  border-radius: var(--radius-ctrl);
  background: var(--color-fill-4);
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.mark-chip .mark-key {
  font-weight: 600;
}

.mark-chip.mark-set {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.count-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 16px;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--color-fill-5);
  color: var(--color-text-muted);
  font-size: 10px;
  font-weight: 500;
}

.seg-card {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px; /* 横纵一致：卡内边距 = 卡间隙 = 元素间距 8 */
  border-radius: var(--radius-ctrl);
  text-align: left;
  transition: background-color var(--duration-fast) var(--ease-out);
  cursor: grab;
}

.seg-card:hover {
  background: var(--color-fill-5);
}

.seg-card.is-selected {
  background: var(--ui-active-fill);
}

.seg-card.is-drop-target {
  box-shadow: inset 0 2px 0 0 var(--color-accent);
}

.seg-card:active {
  cursor: grabbing;
}

.seg-index {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  border-radius: 999px;
  background: var(--color-fill-4);
  color: var(--color-text-muted);
  font-size: 10px;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.seg-card.is-selected .seg-index {
  background: var(--accent-line-soft);
  color: var(--color-accent);
}

.speed-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 20px;
  min-width: 30px;
  padding: 0 5px;
  border-radius: var(--radius-ctrl);
  background: var(--color-fill-4);
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.speed-chip:hover {
  background: var(--color-fill-8);
}

.speed-chip.speed-set {
  background: var(--accent-line-soft);
  color: var(--color-accent);
}

.speed-preset {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 20px;
  min-width: 34px;
  padding: 0 6px;
  border-radius: var(--radius-ctrl);
  background: var(--color-fill-4);
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.speed-preset:hover {
  background: var(--color-fill-8);
  color: var(--color-text-secondary);
}

.speed-preset.is-current {
  background: var(--color-accent);
  color: #fff;
}

.clip-scroll::-webkit-scrollbar {
  width: 8px;
}
.clip-scroll::-webkit-scrollbar-track {
  background: transparent;
}
.clip-scroll::-webkit-scrollbar-thumb {
  background: var(--color-fill-8);
  border-radius: 4px;
  border: 2px solid transparent;
  background-clip: content-box;
}
</style>
