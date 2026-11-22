<template>
  <!-- 剪辑窗口：普通应用窗口（overlay 标题栏），工作台视觉遵循应用设计规范（软面/卡片/主题双轨） -->
  <div class="text-primary bg-canvas flex flex-col h-screen w-screen overflow-hidden">
    <!-- 加载 / 错误 -->
    <div v-if="phase !== 'ready'" class="p-6 flex flex-1 items-center justify-center">
      <BaseEmptyState
        :loading="phase !== 'error'"
        :icon="phase === 'error' ? 'i-ri-error-warning-line' : undefined"
        :title="statusText"
        :description="phase !== 'error' ? loadingDescription : undefined"
      >
        <template v-if="phase === 'error'" #action>
          <BaseButton @click="closeWindow">{{ t('video.clip.exit') }}</BaseButton>
        </template>
      </BaseEmptyState>
    </div>

    <template v-else>
      <!-- 顶部拖动带：overlay 红绿灯安全区（32px），空白 mousedown 拖动窗口 / 双击缩放。
           原生 NSView 叠加方案不可行（WKWebView 独占 hit-test，外来视图收不到事件或
           破坏层级连红绿灯一起吞掉），手势判定只能在 web 层做 -->
      <div class="pb-3 shrink-0 h-8 select-none" @mousedown="onHeaderDrag" />

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
          <div class="px-3 border-t border-divider flex shrink-0 h-10 items-center">
            <div class="flex flex-1 gap-1.5 min-w-0 items-center">
              <span class="text-xs font-medium font-mono tabular-nums">
                {{ formatTimecode(playheadUs, playFps) }}
              </span>
              <span class="text-xs text-muted font-mono tabular-nums"
                >/ {{ formatTimecode(totalUs, playFps) }}</span
              >
            </div>
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
            <div class="flex flex-1 gap-2 min-w-0 items-center justify-end">
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

        <!-- 片段面板：双页签（片段 / 素材）。无总计底栏——成片时长即传输条 total -->
        <aside class="soft-card flex shrink-0 flex-col w-72 overflow-hidden">
          <div class="px-3 border-b border-divider flex shrink-0 gap-1.5 h-10 items-center">
            <BaseButton
              :class="{ '!text-accent': panelTab === 'segments' }"
              @click="panelTab = 'segments'"
            >
              {{ t('video.clip.segments') }}
              <span class="tab-count tabular-nums">{{ segments.length }}</span>
            </BaseButton>
            <BaseButton
              :class="{ '!text-accent': panelTab === 'sources' }"
              @click="panelTab = 'sources'"
            >
              {{ t('video.clip.sources') }}
              <span class="tab-count tabular-nums">{{ clips.length }}</span>
            </BaseButton>
          </div>

          <!-- 片段页：段列表即成片（顺序 = 播放序），拖拽排序；播放到段内时卡底显示进度 -->
          <template v-if="panelTab === 'segments'">
            <div v-if="!segments.length" class="flex flex-1 min-h-0 items-center justify-center">
              <BaseEmptyState icon="i-ri-scissors-cut-line" />
            </div>

            <div
              v-else
              class="clip-scroll px-3 py-3 flex flex-1 flex-col gap-2 min-h-0 overflow-y-auto"
            >
              <div
                v-for="(seg, index) in segments"
                :key="seg.id"
                class="seg-card"
                :class="{
                  'is-selected': index === selectedSegment,
                  'is-dragging': index === panelDragIndex,
                }"
                :data-seg-index="index"
                @click="onSegmentClick(index)"
              >
                <span class="seg-progress" :style="{ width: `${segPlayProgress(index) * 100}%` }" />
                <div class="seg-body">
                  <!-- 序号即拖拽把手（合二为一）：按下拖动排序，点击行为留给卡身 -->
                  <span
                    class="seg-index seg-handle"
                    :title="t('video.clip.dragSort')"
                    @pointerdown="onCardPointerDown($event, index)"
                  >
                    <span class="seg-handle-num tabular-nums">{{ index + 1 }}</span>
                    <!-- 容器与图标分离：图标类自带 display:inline-block，若同一元素
                         兼任绝对定位容器会与其 flex 居中打架（类优先级随注入顺序漂移） -->
                    <span class="seg-handle-grip">
                      <i class="i-ri-draggable text-[10px]" />
                    </span>
                  </span>
                  <div class="seg-main">
                    <div class="seg-line">
                      <span class="seg-range tabular-nums">{{ segmentRangeText(seg) }}</span>
                      <span class="seg-dur tabular-nums">{{
                        formatDuration(segVirtualUs(seg))
                      }}</span>
                    </div>
                    <div class="seg-line">
                      <span class="seg-src">{{ sourceNameOf(seg.sourceIndex) }}</span>
                      <span v-if="seg.speed !== 1" class="seg-speed-tag tabular-nums">{{
                        formatSpeed(seg.speed)
                      }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </template>

          <!-- 素材页：源视频列表（点击跳到该源首个段实例；段已剪空的源置灰不可跳） -->
          <div
            v-else
            class="clip-scroll px-3 py-3 flex flex-1 flex-col gap-2 min-h-0 overflow-y-auto"
          >
            <button
              v-for="(clip, i) in clips"
              :key="clip.meta.path"
              class="seg-card source-row"
              :class="{ 'is-playing': i === playSourceIndex }"
              :disabled="firstSegIndexOf(i) < 0"
              @click="jumpToSource(i)"
            >
              <div class="seg-main">
                <div class="seg-line">
                  <span class="seg-name">{{ sourceNameOf(i) }}</span>
                  <span class="seg-dur tabular-nums">{{
                    formatDuration(clip.meta.durationUs)
                  }}</span>
                </div>
                <div class="seg-line">
                  <span class="seg-src font-mono truncate tabular-nums">{{
                    sourceMetaText(clip)
                  }}</span>
                  <span class="text-[10px] text-muted shrink-0">{{
                    firstSegIndexOf(i) >= 0
                      ? t('video.clip.sourceSegs', { n: segCountOf(i) })
                      : t('video.clip.notOnTimeline')
                  }}</span>
                </div>
              </div>
            </button>
          </div>

          <!-- 底部常驻操作栏（纯图标控件，作用于选中段 = 播放头段）：倍速 = BaseSelect
               触发器插槽（键盘/浮层/外点全复用通用 combobox）；删除 = BaseButton 图标钮 -->
          <div
            class="panel-actions px-3 border-t border-divider flex shrink-0 gap-1.5 h-10 items-center justify-end"
          >
            <BaseSelect
              class="flex-center !px-0 !w-7"
              :model-value="selectedSpeed"
              :options="speedOptions"
              :disabled="selectedSegment < 0"
              :title="t('video.clip.speed')"
              @update:model-value="onSpeedSelect"
            >
              <!-- 触发器插槽：纯图标触发；!w-7 !px-0 flex-center 与相邻 BaseButton
                   图标钮同盒型（方形），soft-chip 边框示意控件身份、无需箭头 -->
              <template #trigger>
                <i class="i-ri-speed-line text-sm flex-none" />
              </template>
            </BaseSelect>
            <BaseButton
              icon="i-ri-delete-bin-line"
              :disabled="selectedSegment < 0"
              :title="t('video.clip.deleteSegment')"
              @click="removeSelectedSegment"
            />
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

      <!-- 状态栏：左侧状态（去停顿审阅 / 导出进度 / 导出结果 / 常态提示）+ 右缘导出入口 -->
      <footer
        class="text-xs text-muted px-3 border-t border-divider flex shrink-0 gap-3 h-10 items-center"
      >
        <template v-if="pendingCuts.length">
          <i class="i-ri-pulse-line text-warning shrink-0" />
          <span class="text-warning">{{ silenceSummary }}</span>
          <span class="shrink-0">{{ t('video.clip.silenceApply') }}</span>
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
        </template>
        <template v-else-if="exportResult">
          <i class="i-ri-checkbox-circle-line text-success shrink-0" />
          <span class="text-success shrink-0">{{ t('video.clip.exported') }}</span>
          <span truncate class="text-secondary">{{ exportResultName }}</span>
          <BaseButton size="inline" @click="revealResult">{{ t('video.clip.reveal') }}</BaseButton>
        </template>
        <template v-else>
          <!-- 常态只留快捷键提示：时间码/入出点已在传输条呈现，底栏不重复 -->
          <span truncate>{{ t('video.clip.hintKeys') }}</span>
        </template>
        <!-- 导出入口（原顶栏迁入）：进行中变取消 -->
        <div class="ml-auto pl-3 flex shrink-0 gap-2 items-center">
          <BaseButton v-if="exporting" size="inline" @click="cancelExport">{{
            t('video.cancel')
          }}</BaseButton>
          <BaseButton
            v-else
            icon="i-ri-export-line"
            :disabled="exportDisabled"
            :title="t('video.clip.export')"
            @click="onExport"
          >
            {{ t('video.clip.export') }}
          </BaseButton>
        </div>
      </footer>

      <!-- 面板拖拽幽灵卡：跟随指针（原卡原地半透明，指示线标落点） -->
      <Teleport to="body">
        <div
          v-if="panelDragPos && segments[panelDragIndex]"
          class="drag-ghost soft-card"
          :style="{ left: `${panelDragPos.x + 14}px`, top: `${panelDragPos.y + 10}px` }"
        >
          <span class="seg-index tabular-nums">{{ panelDragIndex + 1 }}</span>
          <span class="text-[11px] text-secondary font-mono truncate tabular-nums">{{
            segmentRangeText(segments[panelDragIndex])
          }}</span>
          <span class="text-[11px] text-secondary shrink-0 tabular-nums">{{
            formatDuration(segVirtualUs(segments[panelDragIndex]))
          }}</span>
        </div>
        <!-- 插入线：两项之间（accent 满列表宽；末尾追加位在最后一张卡之后） -->
        <div
          v-if="dropLinePos"
          class="drop-line"
          :style="{
            left: `${dropLinePos.x}px`,
            top: `${dropLinePos.y}px`,
            width: `${dropLinePos.w}px`,
          }"
        />
      </Teleport>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Channel, convertFileSrc, invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseProgress from '@/components/ui/BaseProgress.vue'
import { CMD } from '@/commands'
import { showToast } from '@/composables/useToast'
import { formatBytes } from '@/utils/format'
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
/** 选中段 = 播放头所在段（纯派生态：指针在哪哪段选中，无独立选中状态——定位/拖拽/
 * Tab 全部经 seek 驱动，删除/倍速/排序消费此值）。 */
const selectedSegment = computed(() => playSegIndex.value)
const playSeg = computed(() => segments.value[playSegIndex.value] ?? null)
const playFps = computed<FrameRate>(() => playSeg.value?.fps ?? { num: 30, den: 1 })
const playSource = computed<LoadedClip | null>(() => {
  const seg = playSeg.value
  if (!seg) return clips.value[0] ?? null
  return clips.value[seg.sourceIndex] ?? null
})
const previewError = computed(() => {
  if (phase.value !== 'ready') return ''
  if (videoLoadError.value) return t('video.clip.previewFailed')
  const meta = playSource.value?.meta
  if (!meta) return ''
  return meta.previewable
    ? ''
    : t('video.clip.notPreviewable', { codec: meta.codecTag || meta.videoCodec })
})

const playSourceName = computed(() => {
  const path = playSource.value?.meta.path ?? ''
  return path.split('/').pop() || t('video.clip.title')
})
const exportDisabled = computed(() => phase.value !== 'ready' || segments.value.length === 0)
/** 加载态副标题：多素材显示计数（单文件名在多文件下误导），单素材显示文件名。 */
const inputCount = ref(0)
const loadingDescription = computed(() =>
  inputCount.value > 1
    ? t('video.clip.sourcesCount', { n: inputCount.value })
    : playSourceName.value,
)

/** 倍速下拉选项（BaseSelect 图标模式消费，作用于选中段）。 */
const speedOptions = SPEED_PRESETS.map((preset) => ({ label: formatSpeed(preset), value: preset }))

const selectedSpeed = computed(() => segments.value[selectedSegment.value]?.speed ?? 1)

function onSpeedSelect(value: string | number) {
  const index = selectedSegment.value
  if (index >= 0) setSpeed(index, Number(value))
}

/** 源在其首个段实例中的索引（全部段被删则 -1，素材页签该项置灰）。 */
function firstSegIndexOf(sourceIndex: number): number {
  return segments.value.findIndex((seg) => seg.sourceIndex === sourceIndex)
}

/** 源在时间线上的段实例数（素材页签行尾）。 */
function segCountOf(sourceIndex: number): number {
  return segments.value.filter((seg) => seg.sourceIndex === sourceIndex).length
}

/** 素材页签：跳到该源首个段实例并选中（不切页签——监视器即时响应，段卡联动在回片段页时呈现）。 */
function jumpToSource(sourceIndex: number) {
  const index = firstSegIndexOf(sourceIndex)
  if (index < 0) return
  seekVirtual(segStartUs(segments.value, index))
}

// ---------------------------------------------------------------------------
// 面板页签 / 派生辅助
// ---------------------------------------------------------------------------

/** 面板双页签：片段 / 素材。 */
const panelTab = ref<'segments' | 'sources'>('segments')
/** 当前播放段所属源索引（素材页签 is-playing 高亮）。 */
const playSourceIndex = computed(() => playSeg.value?.sourceIndex ?? -1)

/** 段内播放进度（0-1，仅播放头所在段；段卡背景进度条的宽度源）。 */
function segPlayProgress(index: number): number {
  if (index !== playSegIndex.value) return 0
  const seg = segments.value[index]
  if (!seg) return 0
  const start = segStartUs(segments.value, index)
  return Math.min(1, Math.max(0, (playheadUs.value - start) / segVirtualUs(seg)))
}

function formatSpeed(speed: number): string {
  return `${speed}×`
}

function segmentRangeText(seg: TimelineSegment): string {
  // 源时长 < 1h 时剥掉前导 00: 时段，区间显著变短（卡内不被截断的关键一环）
  const underHour = segSourceUs(seg) < 3_600_000_000
  const fmt = (us: number) => {
    const text = formatTimecode(us, seg.fps)
    return underHour ? text.replace(/^00:/, '') : text
  }
  return `${fmt(frameToUs(seg.inFrame, seg.fps))} → ${fmt(frameToUs(seg.outFrame, seg.fps))}`
}

function sourceNameOf(index: number): string {
  const path = clips.value[index]?.meta.path ?? ''
  return path.split('/').pop() ?? ''
}

/** 悬停气泡段信息行的源名截断（窄块辨认行保持气泡紧凑）。 */
function shortenSourceName(name: string, max = 14): string {
  return name.length > max ? `${name.slice(0, max - 1)}…` : name
}

/** 素材行次行元数据（分辨率 · 帧率 · 体积——常态远短于行宽，不产生省略号）。 */
function sourceMetaText(clip: LoadedClip): string {
  const meta = clip.meta
  const fps = meta.fpsNum / meta.fpsDen
  const fpsText = Number.isInteger(fps) ? `${fps}fps` : `${fps.toFixed(2)}fps`
  return `${meta.displayWidth}×${meta.displayHeight} · ${fpsText} · ${formatBytes(meta.sizeBytes, { compact: true })}`
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
  inputCount.value = inputs.length
  if (!inputs.length) {
    fail(t('video.clip.openFailed'))
    return
  }
  // 新一批输入 = 新会话：全部状态重置（窗口复用或页面重载时不得残留上一次的状态）
  playheadUs.value = 0
  inMarkUs.value = null
  outMarkUs.value = null
  exportResult.value = null
  pendingCuts.value = []
  panelTab.value = 'segments'
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
// 时间轴指针交互：纵向分区——标尺带（含播放头手柄）擦洗；轨道区点击即定位 /
// 播放头线附近拖动指针线，其余按住拖拽排序（选中派生于播放头，无独立选中状态）
// ---------------------------------------------------------------------------

/** 标尺带高度（时间刻度 + 擦洗区），与 drawTimeline 同源。 */
const RULER_H = 15
const TRACK_TOP = RULER_H + 4
/** 段块最小可瞄准宽度（px）：低于它的块人手无法稳定点中，命中按中心就近容差。 */
const MIN_SEG_PX = 8
/** 点选与拖拽的位移判定阈值（px）。 */
const DRAG_THRESHOLD_PX = 4
/** 播放头手柄命中宽度（px）：指针落在播放头线两侧该范围内 = 抓指针线而非片段。 */
const PLAYHEAD_GRAB_PX = 8

let scrubbing = false
/** 轨道区段拖拽进行中（悬停反馈让位）。 */
let draggingSeg = false
const hoverUs = ref<number | null>(null)
/** 悬停是否在轨道区（气泡是否附段信息行）。 */
const hoverOnTrack = ref(false)
/** 拖拽排序插入边界（0..n：插到原列表第 k 位之前；-1 = 无 / 原位 no-op）。 */
const dropBoundary = ref(-1)
/** 画布拖拽中的段（块半透明指示；选中已派生，不承担拖拽标识）。 */
const dragSegIndex = ref(-1)
/** 画布拖拽中指针 x（canvas 坐标；-1 = 无）——幽灵块跟随。 */
const dragPointerX = ref(-1)

function ratioToVirtual(canvas: HTMLCanvasElement, clientX: number): number {
  const rect = canvas.getBoundingClientRect()
  const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
  return ratio * totalUs.value
}

interface SegLayout {
  x0: number
  w: number
  centerX: number
}

/** 段块布局：绘制与命中共用同一份累计几何（宽度 ∝ 虚拟时长，杜绝两套游标漂移）。 */
function segLayouts(width: number): SegLayout[] {
  const total = totalUs.value
  if (total <= 0) return []
  let cursor = 0
  return segments.value.map((seg) => {
    const len = segVirtualUs(seg)
    const x0 = (cursor / total) * width
    const w = (len / total) * width
    cursor += len
    return { x0, w, centerX: x0 + w / 2 }
  })
}

/** 轨道区命中。宽块（≥ MIN_SEG_PX）按包含命中，点到即所得；窄块常成串出现（去停顿
 * 碎段仅几像素），包含命中对瞄准误差零容忍，改按 ±MIN_SEG_PX 窗口内中心就近——
 * 宽块不参与就近（其中心可能很远，会抢走本想点中的贴邻窄块）。 */
function segmentAtX(x: number, layout: SegLayout[]): number {
  let contained = -1
  for (let i = 0; i < layout.length; i++) {
    const { x0, w } = layout[i]
    if (x >= x0 && x < x0 + w) {
      contained = i
      break
    }
  }
  if (contained >= 0 && layout[contained].w >= MIN_SEG_PX) return contained
  let best = contained
  let bestDist = contained >= 0 ? Math.abs(x - layout[contained].centerX) : Infinity
  for (let i = 0; i < layout.length; i++) {
    if (layout[i].w >= MIN_SEG_PX) continue
    const dist = Math.abs(x - layout[i].centerX)
    if (dist <= MIN_SEG_PX && dist < bestDist) {
      best = i
      bestDist = dist
    }
  }
  return best
}

/** 拖拽插入边界：最邻近指针 x 的段边界（k = 插到原列表第 k 位之前，n = 末尾）。 */
function nearestBoundary(x: number, layout: SegLayout[], width: number): number {
  let best = 0
  let bestDist = Infinity
  for (let k = 0; k <= layout.length; k++) {
    const bx = k < layout.length ? layout[k].x0 : width
    const dist = Math.abs(bx - x)
    if (dist < bestDist) {
      best = k
      bestDist = dist
    }
  }
  return best
}

function onTimelinePointerDown(event: PointerEvent) {
  const canvas = timelineRef.value
  if (!canvas || !segments.value.length) return
  event.preventDefault() // 拦截拖拽手势的默认选择行为（同段卡把手）
  if (event.clientY - canvas.getBoundingClientRect().top <= TRACK_TOP) {
    startTimelineScrub(canvas, event)
    return
  }
  startSegmentGrab(canvas, event)
}

/** 标尺带擦洗（原全幅行为）：按下即 seek，抬手统一续播 + 重定位预滚。气泡跟随
 * 指针（拖动擦洗也要看得见时间码与段信息）。 */
function startTimelineScrub(canvas: HTMLCanvasElement, event: PointerEvent) {
  scrubbing = true
  updateScrubHover(canvas, event)
  seekVirtual(ratioToVirtual(canvas, event.clientX), isPlaying.value)
  const onMove = (move: PointerEvent) => {
    updateScrubHover(canvas, move)
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

/** 擦洗期间同步悬停指示：气泡随指针移动，轨道区内仍附段信息行。 */
function updateScrubHover(canvas: HTMLCanvasElement, event: PointerEvent) {
  hoverUs.value = ratioToVirtual(canvas, event.clientX)
  hoverOnTrack.value = event.clientY - canvas.getBoundingClientRect().top > TRACK_TOP
  scheduleDraw()
}

/** 轨道区按下：指针落在播放头线 ±PLAYHEAD_GRAB_PX 内 = 抓指针线擦洗；其余位置
 * 点击（未越阈值）= 移动播放指针（选中随播放头派生），拖过阈值 = 拖拽排序（插入线
 * 随最近段边界，drop 复用 moveSegment 纯列表变换，涟漪由列表语义承载；拖动全程
 * 播放头不动）。窄块命中由 segmentAtX 容差保障。 */
function startSegmentGrab(canvas: HTMLCanvasElement, event: PointerEvent) {
  const rect = canvas.getBoundingClientRect()
  const headX = totalUs.value > 0 ? (playheadUs.value / totalUs.value) * canvas.clientWidth : 0
  if (Math.abs(event.clientX - rect.left - headX) <= PLAYHEAD_GRAB_PX) {
    startTimelineScrub(canvas, event) // 抓播放头手柄：拖指针线，不碰片段
    return
  }
  const index = segmentAtX(event.clientX - rect.left, segLayouts(canvas.clientWidth))
  if (index < 0) {
    startTimelineScrub(canvas, event) // 防御：无段可拖时退化为擦洗
    return
  }
  const startX = event.clientX
  const startY = event.clientY
  let dragging = false
  const onMove = (move: PointerEvent) => {
    if (!dragging) {
      if (Math.hypot(move.clientX - startX, move.clientY - startY) < DRAG_THRESHOLD_PX) return
      dragging = true
      draggingSeg = true
      dragSegIndex.value = index
      hoverUs.value = null
    }
    if (dragging) {
      dragPointerX.value = move.clientX - canvas.getBoundingClientRect().left
    }
    const k = nearestBoundary(
      move.clientX - canvas.getBoundingClientRect().left,
      segLayouts(canvas.clientWidth),
      canvas.clientWidth,
    )
    // 原位边界（index / index+1）是 no-op：不画线，不产生「拖了没反应」的错觉
    dropBoundary.value = k !== index && k !== index + 1 ? k : -1
    scheduleDraw()
  }
  const onUp = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    draggingSeg = false
    dragSegIndex.value = -1
    dragPointerX.value = -1
    const k = dropBoundary.value
    dropBoundary.value = -1
    // 点击（未越阈值）= 移动播放指针；拖动 = 只排序片段，播放头全程不动
    if (dragging) {
      if (k >= 0) moveSegment(index, k > index ? k - 1 : k)
    } else {
      seekVirtual(ratioToVirtual(canvas, startX), isPlaying.value)
    }
    scheduleDraw()
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
}

function onTimelineHover(event: PointerEvent) {
  const canvas = timelineRef.value
  if (!canvas || scrubbing || draggingSeg || !segments.value.length) return
  hoverUs.value = ratioToVirtual(canvas, event.clientX)
  hoverOnTrack.value = event.clientY - canvas.getBoundingClientRect().top > TRACK_TOP
  scheduleDraw()
}

function onTimelineLeave() {
  if (hoverUs.value == null) return
  hoverUs.value = null
  hoverOnTrack.value = false
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

  const trackTop = TRACK_TOP
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

    // ── 段块：宽度 ∝ 虚拟时长；每块圆角裁剪 + 细边框（选中的画 accent 边框，
    //     在覆盖层之后单独画）；缩略图按源时间采样铺块（几何与命中共用 segLayouts） ──
    const layout = segLayouts(width)
    segs.forEach((seg, index) => {
      const { x0, w } = layout[index]
      const dragging = index === dragSegIndex.value
      if (dragging) ctx.globalAlpha = 0.55
      ctx.save()
      ctx.beginPath()
      ctx.roundRect(x0, trackTop, w, trackH, 6)
      ctx.clip()
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
      // 倍速角标（≠1 时）
      if (seg.speed !== 1) {
        ctx.fillStyle = accent
        ctx.font = '600 9px ui-monospace, monospace'
        ctx.fillText(formatSpeed(seg.speed), Math.min(x0 + w - 20, x0 + 3), trackTop + 10)
      }
      ctx.restore()
      // 默认细边框（块自身周界，取代旧的段间分隔线；选中的块不画，由覆盖层后的
      // accent 边框接管）
      if (index !== selectedSegment.value) {
        ctx.strokeStyle = cssVar('--color-divider', 'rgba(120,120,160,0.35)')
        ctx.lineWidth = 1
        ctx.beginPath()
        ctx.roundRect(x0 + 0.5, trackTop + 0.5, Math.max(2, w - 1), trackH - 1, 6)
        ctx.stroke()
      }
      ctx.globalAlpha = 1
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

    // ── 选中段描边（窄块最小可视宽度：居中扩展、可压邻块；圆角与轨道槽同心；
    //     选中 = 播放头所在段（派生）） ──
    if (selectedSegment.value >= 0 && selectedSegment.value < segs.length) {
      const sel = layout[selectedSegment.value]
      const drawW = Math.max(MIN_SEG_PX + 2, sel.w)
      ctx.strokeStyle = accent
      ctx.lineWidth = 1.5
      ctx.beginPath()
      ctx.roundRect(sel.centerX - drawW / 2 + 0.75, trackTop + 0.75, drawW - 1.5, trackH - 1.5, 7)
      ctx.stroke()
    }

    // ── 拖拽插入线：accent 竖线 + 顶部旗标（与段卡插入线同语义） ──
    if (dropBoundary.value >= 0 && dropBoundary.value <= segs.length) {
      const bx = dropBoundary.value < segs.length ? layout[dropBoundary.value].x0 : width
      ctx.strokeStyle = accent
      ctx.lineWidth = 2
      ctx.beginPath()
      ctx.moveTo(bx - 0.5, guideTop)
      ctx.lineTo(bx - 0.5, trackBottom)
      ctx.stroke()
      ctx.fillStyle = accent
      ctx.beginPath()
      ctx.moveTo(bx - 4, guideTop)
      ctx.lineTo(bx + 3, guideTop)
      ctx.lineTo(bx, guideTop + 6)
      ctx.closePath()
      ctx.fill()
    }

    // ── 拖拽幽灵块：跟随指针居中（钳制轨道内；原块原地半透明，插入线标落点） ──
    if (dragSegIndex.value >= 0 && dragSegIndex.value < segs.length && dragPointerX.value >= 0) {
      const src = layout[dragSegIndex.value]
      const gx = Math.min(width - src.w, Math.max(0, dragPointerX.value - src.w / 2))
      ctx.globalAlpha = 0.75
      ctx.fillStyle = cssVar('--color-fill-5', 'rgba(120,120,130,0.18)')
      ctx.beginPath()
      ctx.roundRect(gx, trackTop, src.w, trackH, 6)
      ctx.fill()
      ctx.strokeStyle = accent
      ctx.lineWidth = 1.5
      ctx.beginPath()
      ctx.roundRect(gx + 0.75, trackTop + 0.75, Math.max(2, src.w - 1.5), trackH - 1.5, 6)
      ctx.stroke()
      ctx.globalAlpha = 1
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
    for (let sec = 0; sec <= seconds; sec += step) {
      const x = (sec / seconds) * width
      if (x > width - 26) break
      ctx.fillStyle = muted
      ctx.fillText(formatClock(sec), x + 2, 9)
      ctx.strokeStyle = cssVar('--color-divider', 'rgba(120,120,160,0.1)')
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.moveTo(x + 0.5, 10)
      ctx.lineTo(x + 0.5, 14)
      ctx.stroke()
    }

    // ── 悬停指示：muted 虚线 + 深色实底时间码气泡（擦洗时气泡跟随指针，虚线省略——
    //     播放头已在指针处，虚线与之重合没有信息量） ──
    if (hoverUs.value != null) {
      const x = Math.min(width, Math.max(0, usToX(hoverUs.value)))
      if (!scrubbing) {
        ctx.strokeStyle = cssVar('--color-text-muted', muted)
        ctx.lineWidth = 1
        ctx.setLineDash([3, 3])
        ctx.beginPath()
        ctx.moveTo(x + 0.5, guideTop)
        ctx.lineTo(x + 0.5, guideBottom)
        ctx.stroke()
        ctx.setLineDash([])
      }
      const label = formatTimecode(hoverUs.value, playFps.value)
      // 轨道区悬停附段信息行（窄块辨认：序号 + 源名）
      let segLine = ''
      if (hoverOnTrack.value) {
        const idx = segmentAtX(x, layout)
        const seg = idx >= 0 ? segs[idx] : null
        if (seg) segLine = `${idx + 1}·${shortenSourceName(sourceNameOf(seg.sourceIndex))}`
      }
      ctx.font = '9px ui-monospace, monospace'
      const tw = Math.max(ctx.measureText(label).width, ctx.measureText(segLine).width)
      const bh = segLine ? 27 : 14
      const bx = Math.min(width - tw - 12, Math.max(2, x - tw / 2 - 4))
      // 气泡贴轨道底部浮在缩略图上：深色实底（浮在任意明暗缩略图上恒可读——
      // 监视器行业惯例，主题无关，同媒体井例外）
      const by = trackBottom - bh - 4
      ctx.fillStyle = '#24262b'
      ctx.strokeStyle = 'rgba(255,255,255,0.14)'
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.roundRect(bx + 0.5, by + 0.5, tw + 7, bh - 1, 4)
      ctx.fill()
      ctx.stroke()
      ctx.fillStyle = 'rgba(255,255,255,0.92)'
      ctx.fillText(label, bx + 4, by + 10)
      if (segLine) {
        ctx.fillStyle = 'rgba(255,255,255,0.6)'
        ctx.fillText(segLine, bx + 4, by + 21)
      }
    }
  }
}

function formatClock(totalSeconds: number): string {
  const m = Math.floor(totalSeconds / 60)
  const s = Math.floor(totalSeconds % 60)
  return `${m}:${s < 10 ? '0' : ''}${s}`
}

// ---------------------------------------------------------------------------
// 打点 / 删除 / 切分 / 排序（三大核心操作）；选中 = 播放头段（派生）
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

/** 删除打点区间（可跨段/切半段，余下涟漪前移），播放头停在删除起点（⌫ 统一删除入口）。 */
function deleteMarkedRange() {
  const start = inMarkUs.value
  const end = outMarkUs.value
  if (start == null || end == null || end <= start) {
    showToast(t('video.clip.needRange'), { kind: 'error' })
    return
  }
  segments.value = deleteVirtualRange(segments.value, start, end)
  inMarkUs.value = null
  outMarkUs.value = null
  syncAfterEdit(start)
}

function removeSegmentAt(index: number) {
  if (index < 0 || index >= segments.value.length) return
  const at = segStartUs(segments.value, index)
  segments.value.splice(index, 1)
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
  syncAfterEdit(playheadUs.value)
}

function onSegmentClick(index: number) {
  if (suppressCardClick) return // 拖拽收尾的那次 click，不是跳转意图
  seekVirtual(segStartUs(segments.value, index)) // 跳段首；选中随播放头派生
}

// ---------------------------------------------------------------------------
// 拖拽排序（段卡）：仅经序号把手（pointerdown）触发；选中派生于播放头，无独立状态
// ---------------------------------------------------------------------------

/** 面板拖拽插入边界（0..n，-1 = 无 / 原位）与指示线位置（viewport 坐标）。 */
const panelDropBoundary = ref(-1)
const dropLinePos = ref<{ x: number; y: number; w: number } | null>(null)
/** 面板拖拽幽灵跟随点（viewport 坐标；null = 无拖拽）。 */
const panelDragPos = ref<{ x: number; y: number } | null>(null)
/** 面板拖拽中的段卡。 */
const panelDragIndex = ref(-1)
/** 拖拽刚结束的一次 click 抑制（pointerup 在原卡上仍派发 click，会把播放头拽到段首）。 */
let suppressCardClick = false

/** 由指针 Y 求插入边界：首个中点在指针下方的卡 = 该卡之前（0..n，n = 末尾追加位），
 * 覆盖卡间 8px 间隙与列表上下沿；并定位两项之间的指示线。原位边界（自身 / 紧邻下
 * 一位）不画线，与时间轴同纪律。 */
function updatePanelDropLine(clientY: number, from: number) {
  const cards = Array.from(document.querySelectorAll<HTMLElement>('[data-seg-index]'))
  const list = document.querySelector<HTMLElement>('.clip-scroll')
  if (!cards.length || !list) {
    panelDropBoundary.value = -1
    dropLinePos.value = null
    return
  }
  let k = cards.length
  for (const el of cards) {
    const r = el.getBoundingClientRect()
    if (clientY < r.top + r.height / 2) {
      k = Number(el.dataset.segIndex)
      break
    }
  }
  if (k === from || k === from + 1) {
    panelDropBoundary.value = -1
    dropLinePos.value = null
    return
  }
  const anchor = k < cards.length ? cards[k]! : cards[cards.length - 1]!
  const ar = anchor.getBoundingClientRect()
  const lr = list.getBoundingClientRect()
  panelDropBoundary.value = k
  dropLinePos.value = {
    x: lr.left,
    w: lr.width,
    y: k < cards.length ? ar.top - 5 : ar.bottom + 3, // 居中于卡间 8px 间隙
  }
}

function onCardPointerDown(event: PointerEvent, index: number) {
  if (event.button !== 0) return // 仅挂在序号把手上（卡身留给点击跳转）
  // preventDefault 掐掉选择机制起点：user-select:none 之外，WebKit 拖拽手势仍可能
  // 拉出选区（拖动中跨元素选择），pointerdown 默认行为拦截是最彻底的防线
  event.preventDefault()
  const startY = event.clientY
  let dragging = false
  const onMove = (move: PointerEvent) => {
    if (!dragging) {
      if (Math.abs(move.clientY - startY) < DRAG_THRESHOLD_PX) return
      dragging = true
      panelDragIndex.value = index
    }
    // 幽灵卡跟随指针（原卡原地半透明）；插入线随边界（两项之间 / 末尾追加位）
    panelDragPos.value = { x: move.clientX, y: move.clientY }
    updatePanelDropLine(move.clientY, index)
  }
  const onUp = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    panelDragIndex.value = -1
    panelDragPos.value = null
    dropLinePos.value = null
    const boundary = panelDropBoundary.value
    panelDropBoundary.value = -1
    if (!dragging) return // 未越阈值 = 纯点击，交给 @click
    suppressCardClick = true
    window.setTimeout(() => (suppressCardClick = false), 0)
    if (boundary >= 0) {
      // 边界 k = 插到原列表第 k 位之前（与时间轴插入线同语义）；下向拖拽移除后前移一位
      moveSegment(index, index < boundary ? boundary - 1 : boundary)
    }
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
}

/** 移动段到新位置（涟漪：列表重排天然承载）。 */
function moveSegment(from: number, to: number) {
  if (from === to || from < 0 || to < 0 || from >= segments.value.length) return
  const next = [...segments.value]
  const [item] = next.splice(from, 1)
  next.splice(to, 0, item)
  segments.value = next
  syncAfterEdit(playheadUs.value)
}

function setSpeed(segIndex: number, speed: number) {
  const seg = segments.value[segIndex]
  if (!seg) return
  segments.value = segments.value.map((s, i) => (i === segIndex ? { ...s, speed } : s))
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

/** 头部空白区：按下后位移越阈值 = 拖动窗口（overlay 无原生标题栏）。拖动必须延迟到
 *  位移越阈值才 startDragging——首次按下即拖动会让系统拖拽会话吞掉后续鼠标事件。
 *  双击用 mousedown 手动计时判定（WKWebView 的 mousedown event.detail 恒为 1、
 *  dblclick 合成不可靠，均不能用作判定；延迟拖动后第二次 mousedown 必达页面，
 *  计时可靠）；缩放底层走 AppKit 原生 zoom（tao 对可缩放窗口直调 NSWindow.zoom）。 */
let lastHeaderDownMs = 0

function onHeaderDrag(event: MouseEvent) {
  if (event.button !== 0) return
  if ((event.target as HTMLElement).closest('button')) return
  event.preventDefault() // 拦截拖拽手势的默认选择行为
  const now = performance.now()
  if (now - lastHeaderDownMs < 400) {
    lastHeaderDownMs = 0
    onHeaderZoom()
    return // 双击不进拖动
  }
  lastHeaderDownMs = now
  const startX = event.clientX
  const startY = event.clientY
  let dragging = false
  const onMove = (move: PointerEvent) => {
    if (!dragging) {
      if (Math.hypot(move.clientX - startX, move.clientY - startY) < DRAG_THRESHOLD_PX) return
      dragging = true
      cleanup()
      void getCurrentWindow()
        .startDragging()
        .catch(() => {})
    }
  }
  const cleanup = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', cleanup)
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', cleanup)
}

function onHeaderZoom() {
  void invoke(CMD.videoClipZoom).catch(() => {})
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
      // 删除键统一删：优先打点区间（I/O 已设），否则选中段（= 播放头段）
      if (inMarkUs.value != null && outMarkUs.value != null) deleteMarkedRange()
      else removeSelectedSegment()
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
      // Tab 段间跳转（Shift 反向）：播放头移到段首，选中随播放头派生（倍速/删除/排序的锚）
      {
        const n = segments.value.length
        if (n) {
          const cur = selectedSegment.value
          const next = event.shiftKey ? (cur - 1 + n) % n : (cur + 1) % n
          seekVirtual(segStartUs(segments.value, next))
        }
      }
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
// 检查器滚动跟随：选中 = 播放头段（派生），列表随播放/定位/Tab 跳段滚动跟随
// （block:nearest 只在出视野时滚动，窄块兜底）
watch(selectedSegment, async (index) => {
  if (index < 0) return
  await nextTick()
  document.querySelector(`[data-seg-index="${index}"]`)?.scrollIntoView({ block: 'nearest' })
})
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

/* 段卡：圆角 + 默认细边框；选中 = accent 边框（无填充底色，播放指示由进度底承载） */
.seg-card {
  position: relative; /* 进度底 / 把手图标的定位锚 */
  overflow: hidden; /* 浮层随卡圆角裁剪 */
  width: 100%;
  padding: 8px; /* 横纵一致：卡内边距 = 卡间隙 = 元素间距 8 */
  border: 1px solid var(--color-divider);
  border-radius: var(--radius-ctrl);
  text-align: left;
  transition: background-color var(--duration-fast) var(--ease-out);
  cursor: pointer; /* 卡身可点击（跳段/跳源）；拖拽光标只属于序号把手（.seg-handle） */
  user-select: none; /* 防误选文本 */
}

/* 段卡行内容（相对定位压在进度底之上：DOM 序在进度底层之后） */
.seg-body {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 主信息列：两行——关键行（序号 + 源区间 + 成片时长）与次行（源名 + 倍速标记） */
.seg-main {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.seg-line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.seg-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  color: var(--color-text-secondary);
}

.seg-range {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--color-text-secondary);
}

/* 右缘指标（时长等）：不被挤压截断 */
.seg-dur {
  flex-shrink: 0;
  margin-left: auto;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--color-text-secondary);
}

.seg-src {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  color: var(--color-text-muted);
}

/* 倍速标记（≠1 时次行右缘；编辑入口在底部操作栏倍速 select） */
.seg-speed-tag {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--color-accent);
}

/* 面板拖拽插入线：两项之间（与时间轴插入线同语义），accent 满列表宽 */
.drop-line {
  position: fixed;
  z-index: 50;
  height: 2px;
  border-radius: 1px;
  background: var(--color-accent);
  pointer-events: none;
}

/* 拖拽幽灵卡：跟随指针的紧凑单行卡影（accent 边框示「被拖的是这张」） */
.drag-ghost {
  position: fixed;
  z-index: 50;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 232px;
  padding: 6px 10px;
  border: 1px solid var(--color-accent);
  border-radius: var(--radius-ctrl);
  opacity: 0.9;
  pointer-events: none;
}

/* 播放进度底：播放头所在段专属，宽度 = 段内进度（accent 浅染，不与选中底冲突） */
.seg-progress {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  background: var(--accent-line-soft);
  pointer-events: none;
}

/* 页签计数：浅色纯文本（页签本体 = BaseButton ghost + active 接管选中态） */
.tab-count {
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
}

.seg-card:hover {
  background: var(--color-fill-5);
}

.seg-card.is-selected {
  border-color: var(--color-accent);
}

/* 素材行：不可拖拽（指针可点跳转）；播放源指示 = accent 边框（段卡选中同款观感，
 * 段卡的播放指示仅由进度底承载，与选中态解耦） */

.source-row.is-playing {
  border-color: var(--color-accent);
}

/* 拖拽中的段卡半透明（拖拽标识与选中态解耦） */
.seg-card.is-dragging {
  opacity: 0.55;
}

.seg-card:disabled {
  cursor: default;
}

.seg-index {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px; /* 长数字自适应：min-width + 横向 padding 撑开，不定宽不截断 */
  height: 16px;
  padding: 0 4px;
  border-radius: 4px;
  background: var(--color-fill-4);
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 9px;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

/* 序号即拖拽把手：卡身 hover 时序号淡出、拖拽把手浮现（可拖暗示）；
 * 按下抓取。两态绝对叠放交叉淡入，盒型恒定不跳动 */
.seg-handle {
  position: relative;
  align-self: center;
  cursor: grab;
}

.seg-handle-grip {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.seg-handle-num {
  transition: opacity var(--duration-fast) var(--ease-out);
}

.seg-card:hover .seg-handle-num,
.seg-card.is-dragging .seg-handle-num {
  opacity: 0;
}

.seg-card:hover .seg-handle-grip,
.seg-card.is-dragging .seg-handle-grip {
  opacity: 1;
}

.seg-handle:active {
  cursor: grabbing;
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
