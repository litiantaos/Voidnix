<template>
  <div class="flex-col-full-pb">
    <!-- ── 移除背景：预览区 ── -->
    <Transition :css="false" v-bind="expandHooks">
      <div v-if="tool === 'removeBg' && previewUrl" p="x-3 b-3" shrink="0" flex="~ col" gap="2">
        <div
          class="checkerboard border border-divider radius-panel border-solid"
          relative
          shrink="0"
          h="48"
          overflow="hidden"
        >
          <img
            v-if="originalPreview"
            :src="originalPreview"
            class="h-full w-full inset-0 absolute object-contain"
            :style="{ opacity: result ? 0 : 1, transition: 'opacity 600ms ease-in-out' }"
            :alt="t('image.original')"
          />
          <Transition name="result-fade">
            <img
              v-if="result"
              :src="result.previewDataUrl"
              class="h-full w-full inset-0 absolute object-contain"
              :alt="t('image.result')"
            />
          </Transition>
        </div>
      </div>
    </Transition>

    <!-- ── 拼接：实时预览 = 列表合二为一 ── -->
    <Transition :css="false" v-bind="expandHooks">
      <div
        v-if="tool === 'stitch' && stitchFiles.length"
        p="x-3 b-3"
        shrink="0"
        flex="~ col"
        gap="2"
      >
        <div
          class="hide-scrollbar border border-divider radius-panel border-solid fill-ctrl"
          relative
          shrink="0"
          :style="[previewStyle, overflowStyle]"
          @click="selectedFile = -1"
        >
          <div
            class="h-full"
            :class="stitchDirection === 'vertical' ? 'flex flex-col' : 'flex flex-row items-center'"
          >
            <div
              v-for="(file, i) in stitchFiles"
              :key="file"
              shrink="0"
              class="cursor-pointer relative"
              :class="[
                stitchDirection === 'vertical' ? 'w-1/2 self-center' : 'h-full',
                selectedFile === i ? 'stitch-selected' : '',
              ]"
              :style="itemStyle(i)"
              @click.stop="selectedFile = selectedFile === i ? -1 : i"
            >
              <img
                v-if="thumbCache.get(file)"
                :src="thumbCache.get(file)"
                class="block"
                :class="
                  stitchDirection === 'vertical' ? 'w-full h-auto' : 'h-full w-auto max-w-none'
                "
                draggable="false"
                alt=""
              />
              <div v-else class="flex-center h-12 w-full">
                <i class="i-ri-loader-4-line text-sm text-muted animate-spin"></i>
              </div>
              <span
                class="text-xs text-white px-0.5 bg-black/40 left-0 top-0 absolute"
                style="z-index: 1"
                >{{ i + 1 }}</span
              >
            </div>
          </div>
        </div>
      </div>
    </Transition>

    <BaseSettingsList :items="items" @execute="onExecute">
      <!-- 移除背景：source 行 -->
      <template v-if="tool === 'removeBg'" #trailing-source>
        <div flex gap="2">
          <BaseButton :disabled="processing" @click.stop="pickInput">{{
            t('image.select')
          }}</BaseButton>
          <BaseButton v-if="processing" disabled>{{ t('image.processing') }}</BaseButton>
          <BaseButton
            v-else-if="inputPath && !result"
            variant="primary"
            :disabled="processing"
            @click.stop="removeBg"
            >{{ t('image.removeBg') }}</BaseButton
          >
        </div>
      </template>

      <!-- 拼接：source 行 -->
      <template v-else #trailing-source>
        <div flex gap="2">
          <BaseButton :disabled="processing" @click.stop="pickStitchFiles">{{
            t('image.add')
          }}</BaseButton>
        </div>
      </template>

      <!-- 间距：数字输入 -->
      <template v-if="tool === 'stitch'" #trailing-gap>
        <BaseInput
          :model-value="String(stitchGap)"
          type="number"
          class="text-center w-16"
          @update:model-value="onGapInput"
        />
      </template>

      <template #trailing-outputDir>
        <div flex gap="2">
          <BaseButton v-if="config.outputDir" @click.stop="resetOutputDir">{{
            t('image.sameDir')
          }}</BaseButton>
          <BaseButton @click.stop="pickOutputDir">{{ t('image.select') }}</BaseButton>
        </div>
      </template>
    </BaseSettingsList>
  </div>
</template>

<script setup lang="ts">
import { computed, onDeactivated, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { currentMonitor } from '@tauri-apps/api/window'
import { CMD } from '@/commands'
import { useAppStore, withSuppressBlur } from '@/stores/app'
import { isTauri, hideWindow } from '@/utils/tauri'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import { t } from '@/runtime/i18n'
import type { SettingItem } from '@/types/settings'
import { config } from './config'
import { stitchFiles, tool, pendingInputPath } from './index'
import {
  IMAGE_EXTENSIONS,
  RESIZE_PRESETS,
  displayPath,
  fileNameFromPath,
  formatBytes,
  buildOutputPath,
  type ImageResult,
  type Resize,
  type StitchDirection,
} from './logic'

const appStore = useAppStore()

// ── 高度展开过渡：v-if 块平滑伸缩（maxHeight 0 → scrollHeight），避免下方布局跳变 ──
// 曲线/时长统一走设计基元（--ease-* / --duration-*），不发明新值。
// scrollHeight 不受 maxHeight 约束，始终反映完整内容高度；结束后清空内联样式恢复自然流。
// 回调定时常量与 theme.css 的 --duration-* 同步（改 CSS 时一并更新）：
const EXPAND_MS = 200 // = --duration-normal
const COLLAPSE_MS = 150 // = --duration-fast
const expandHooks = {
  onBeforeEnter(el: Element) {
    const e = el as HTMLElement
    e.style.maxHeight = '0'
    e.style.opacity = '0'
    e.style.overflow = 'hidden'
  },
  onEnter(el: Element, done: () => void) {
    const e = el as HTMLElement
    e.style.transition =
      'max-height var(--duration-normal) var(--ease-spring), opacity var(--duration-fast) var(--ease-out)'
    void e.offsetHeight // 强制 reflow：提交 maxHeight:0 起始态后再过渡到目标高度
    e.style.maxHeight = `${e.scrollHeight}px`
    e.style.opacity = '1'
    setTimeout(done, EXPAND_MS)
  },
  onAfterEnter(el: Element) {
    clearExpandStyles(el as HTMLElement)
  },
  onBeforeLeave(el: Element) {
    const e = el as HTMLElement
    e.style.maxHeight = `${e.scrollHeight}px`
    e.style.opacity = '1'
    e.style.overflow = 'hidden'
  },
  onLeave(el: Element, done: () => void) {
    const e = el as HTMLElement
    void e.offsetHeight // 强制 reflow：提交起始高度后再过渡到 0
    e.style.transition =
      'max-height var(--duration-fast) var(--ease-in), opacity var(--duration-fast) var(--ease-in)'
    e.style.maxHeight = '0'
    e.style.opacity = '0'
    setTimeout(done, COLLAPSE_MS)
  },
  onAfterLeave(el: Element) {
    clearExpandStyles(el as HTMLElement)
  },
}

function clearExpandStyles(e: HTMLElement) {
  e.style.maxHeight = ''
  e.style.transition = ''
  e.style.overflow = ''
  e.style.opacity = ''
}

const processing = ref(false)
const result = ref<ImageResult | null>(null)
const savedOutputPath = ref('')

// ── 移除背景 ──
const inputPath = ref('')
const originalPreview = ref('')

// ── 拼接 ──
const stitchDirection = ref<StitchDirection>('vertical')
const stitchGap = ref(0)
const stitchResize = ref<number>(RESIZE_PRESETS[0])
const selectedFile = ref(-1)
/// 拼接缩略图 LRU 缓存：base64 data URL 单张可达数十 KB，无上限时拼接大量图片致内存膨胀
const THUMB_CACHE_MAX = 20
const thumbCache = ref<Map<string, string>>(new Map())

// ── 屏幕高度（预览区 = 75%） ──
const screenHeight = ref(800)

if (isTauri) {
  currentMonitor()
    .then((m) => {
      if (m) screenHeight.value = m.size.height / m.scaleFactor
    })
    .catch(() => {})
}

const previewStyle = computed(() => {
  if (tool.value !== 'stitch') return {}
  const ratio = stitchDirection.value === 'vertical' ? 0.35 : 0.25
  return { height: `${Math.round(screenHeight.value * ratio)}px` }
})

// 横向仅横向滚动，纵向仅纵向滚动
const overflowStyle = computed(() =>
  stitchDirection.value === 'vertical'
    ? ({ overflowX: 'hidden', overflowY: 'auto' } as const)
    : ({ overflowX: 'auto', overflowY: 'hidden' } as const),
)

const previewUrl = computed(() => (originalPreview.value || result.value?.previewDataUrl) ?? '')

/**
 * 每张图的间距/重叠样式 + z-index。
 * gap > 0：正值 margin（间距）；gap < 0：负值 margin（重叠）。
 * 重叠模式（如电影台词拼接）：序号小的在上层，首张图完整显示，
 * 后续图上半部分被前一张覆盖，仅露出底部台词。Rust 端逆序绘制对齐。
 */
function itemStyle(index: number): Record<string, string> {
  const g = stitchGap.value
  const isVertical = stitchDirection.value === 'vertical'
  const last = index === stitchFiles.value.length - 1
  const style: Record<string, string> = {
    zIndex: String(stitchFiles.value.length - index),
  }
  if (g === 0 || last) return style
  const key = isVertical ? 'marginBottom' : 'marginRight'
  style[key] = `${g}px`
  return style
}

// ── 重置 ──

function reset() {
  result.value = null
  originalPreview.value = ''
  inputPath.value = ''
  savedOutputPath.value = ''
  selectedFile.value = -1
  stitchFiles.value = []
  thumbCache.value.clear()
}

// 工具切换：清空当前状态
watch(tool, () => {
  result.value = null
  originalPreview.value = ''
  inputPath.value = ''
  savedOutputPath.value = ''
  selectedFile.value = -1
})

// 退出扩展（KeepAlive deactivate）：自动清空重置
onDeactivated(() => {
  reset()
  tool.value = 'removeBg'
})

// 跨扩展进入：finder-ext 等经事件总线投递的待处理图片路径，写入即加载。
// 时序同 video 扩展：emit 走 IPC 往返（macrotask），setActiveExtension 同步改 ref 触发
// Vue flush（microtask），microtask 必先于 macrotask 清空，故 View 挂载 + watch 注册恒先于
// IPC 回调到达，watch 不会漏触发。onDeactivated 已将 tool 复位为 removeBg，此处确保即可。
watch(pendingInputPath, (path) => {
  if (!path) return
  pendingInputPath.value = ''
  tool.value = 'removeBg'
  void setInput(path)
})

// ── 缩略图加载 ──

watch(
  stitchFiles,
  async (files) => {
    for (const file of files) {
      if (thumbCache.value.has(file)) continue
      try {
        const url = await invoke<string>(CMD.imageReadPreview, { inputPath: file })
        if (thumbCache.value.size >= THUMB_CACHE_MAX) {
          const first = thumbCache.value.keys().next().value
          if (first) thumbCache.value.delete(first)
        }
        thumbCache.value.set(file, url)
      } catch {
        /* ignore */
      }
    }
  },
  { immediate: true, deep: true },
)

// ── 设置列表 ──

const resizeTitle = computed(() =>
  stitchDirection.value === 'vertical' ? t('image.width') : t('image.height'),
)

const resizeOptions = computed(() => RESIZE_PRESETS.map((v) => ({ label: String(v), value: v })))

const removeBgSourceSubtitle = computed(() => {
  if (processing.value) return t('image.segmentingForeground')
  if (!inputPath.value) return t('image.formatsHint')
  if (result.value) {
    return `${result.value.width}×${result.value.height} · ${formatBytes(result.value.sizeBytes)} · PNG`
  }
  return displayPath(inputPath.value)
})

const items = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []

  // ── 操作（列表最前、紧贴预览区；原按钮组改为列表项，回车触发；无图标）──
  // 选中图片时只显示上移/下移/移除；未选中时显示结果操作（复制/保存/访达）
  if (tool.value === 'stitch' && selectedFile.value >= 0) {
    const isVertical = stitchDirection.value === 'vertical'
    list.push(
      {
        id: 'act-up',
        title: isVertical ? t('image.moveUp') : t('image.moveLeft'),
        type: 'action',
        action: moveUp,
        group: t('image.group.actions'),
      },
      {
        id: 'act-down',
        title: isVertical ? t('image.moveDown') : t('image.moveRight'),
        type: 'action',
        action: moveDown,
        group: t('image.group.actions'),
      },
      {
        id: 'act-remove',
        title: t('image.remove'),
        type: 'action',
        action: removeSelected,
        group: t('image.group.actions'),
        tone: 'danger',
      },
    )
  } else {
    const hasResult =
      (tool.value === 'removeBg' && !!result.value) ||
      (tool.value === 'stitch' && stitchFiles.value.length >= 2)
    if (hasResult) list.push(...resultActionItems())
  }

  if (tool.value === 'removeBg') {
    list.push({
      id: 'source',
      title: inputPath.value ? fileNameFromPath(inputPath.value) : t('image.inputImage'),
      subtitle: removeBgSourceSubtitle.value,
      type: 'custom',
      group: t('image.group.file'),
    })
  } else {
    list.push({
      id: 'source',
      title: t('image.inputImage'),
      subtitle:
        stitchFiles.value.length > 0
          ? t('image.imageCount', { count: stitchFiles.value.length })
          : undefined,
      type: 'custom',
      group: t('image.group.file'),
    })
    list.push(
      {
        id: 'direction',
        title: t('image.direction'),
        type: 'select',
        value: stitchDirection.value,
        options: [
          { label: t('image.direction.vertical'), value: 'vertical' },
          { label: t('image.direction.horizontal'), value: 'horizontal' },
        ],
        update: (v) => {
          stitchDirection.value = v as StitchDirection
        },
        group: t('image.group.params'),
      },
      {
        id: 'resize',
        title: resizeTitle.value,
        type: 'select',
        value: stitchResize.value,
        options: resizeOptions.value,
        update: (v) => {
          stitchResize.value = v as number
        },
        group: t('image.group.params'),
      },
      {
        id: 'gap',
        title: t('image.gap'),
        type: 'custom',
        group: t('image.group.params'),
      },
    )
  }

  list.push({
    id: 'outputDir',
    title: t('image.outputDir'),
    subtitle: config.outputDir ? displayPath(config.outputDir) : t('image.sameAsSource'),
    type: 'custom',
    group: t('image.group.output'),
  })

  return list
})

/** 结果操作项（复制 / 保存 / 在访达中显示）：移除背景与拼接共用，供 items 复用。 */
function resultActionItems(): SettingItem[] {
  const ops: SettingItem[] = [
    {
      id: 'act-copy',
      title: t('image.copy'),
      type: 'action',
      action: copyToClipboard,
      group: t('image.group.actions'),
    },
    {
      id: 'act-save',
      title: t('image.save'),
      type: 'action',
      action: saveToFile,
      group: t('image.group.actions'),
    },
  ]
  if (savedOutputPath.value) {
    ops.push({
      id: 'act-reveal',
      title: t('image.revealInFinder'),
      type: 'action',
      action: revealInFinder,
      group: t('image.group.actions'),
    })
  }
  return ops
}

function onExecute(item: SettingItem) {
  if (item.id === 'outputDir') {
    void pickOutputDir()
    return
  }
  if (item.id === 'source') {
    if (tool.value === 'removeBg') {
      if (processing.value) return
      if (inputPath.value) void removeBg()
      else void pickInput()
    } else {
      void pickStitchFiles()
    }
  }
}

function onGapInput(val: string) {
  const n = parseInt(val, 10)
  stitchGap.value = Number.isFinite(n) ? n : 0
}

// ── 文件排序 ──

function sortByFileName(files: string[]): string[] {
  return [...files].sort((a, b) => fileNameFromPath(a).localeCompare(fileNameFromPath(b)))
}

// ── 移除背景 ──

async function loadPreview(path: string) {
  try {
    originalPreview.value = await invoke<string>(CMD.imageReadPreview, { inputPath: path })
  } catch {
    originalPreview.value = ''
  }
}

async function pickInput() {
  await withSuppressBlur(async () => {
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: false,
      allowedExtensions: [...IMAGE_EXTENSIONS],
    })
    if (paths[0]) await setInput(paths[0])
  })
}

async function setInput(path: string) {
  inputPath.value = path
  result.value = null
  savedOutputPath.value = ''
  await loadPreview(path)
}

async function removeBg() {
  if (!inputPath.value || processing.value) return
  processing.value = true
  result.value = null
  try {
    result.value = await invoke<ImageResult>(CMD.imageRemoveBg, {
      inputPath: inputPath.value,
    })
  } catch (e) {
    appStore.showStatus(`${t('image.processFailed')}：${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  } finally {
    processing.value = false
  }
}

// ── 拼接 ──

async function pickStitchFiles() {
  await withSuppressBlur(async () => {
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: true,
      allowedExtensions: [...IMAGE_EXTENSIONS],
    })
    if (paths.length) {
      result.value = null
      savedOutputPath.value = ''
      const seen = new Set(stitchFiles.value)
      const fresh = paths.filter((p) => !seen.has(p))
      if (stitchFiles.value.length === 0) {
        // 首次添加：按文件名升序
        stitchFiles.value = sortByFileName(fresh)
      } else {
        // 后续添加：追加到末尾
        stitchFiles.value = [...stitchFiles.value, ...fresh]
      }
    }
  })
}

function moveUp() {
  const i = selectedFile.value
  if (i <= 0) return
  const list = stitchFiles.value
  ;[list[i - 1], list[i]] = [list[i], list[i - 1]]
  selectedFile.value = i - 1
}

function moveDown() {
  const i = selectedFile.value
  const list = stitchFiles.value
  if (i >= list.length - 1) return
  ;[list[i + 1], list[i]] = [list[i], list[i + 1]]
  selectedFile.value = i + 1
}

function removeSelected() {
  const i = selectedFile.value
  if (i < 0) return
  stitchFiles.value.splice(i, 1)
  selectedFile.value = -1
}

/** 生成拼接结果（复制/保存时惰性调用，文件或参数变更后自动重新生成）。 */
async function ensureStitched(): Promise<ImageResult | null> {
  if (stitchFiles.value.length < 2) return null
  // 已有结果且指纹未变：复用
  const fp = stitchFingerprint()
  if (result.value && resultFingerprint === fp) return result.value
  processing.value = true
  try {
    const resize: Resize =
      stitchDirection.value === 'vertical'
        ? { mode: 'width', value: stitchResize.value }
        : { mode: 'height', value: stitchResize.value }
    const r = await invoke<ImageResult>(CMD.imageStitch, {
      inputPaths: stitchFiles.value,
      direction: stitchDirection.value,
      gap: stitchGap.value,
      resize,
    })
    result.value = r
    resultFingerprint = fp
    return r
  } catch (e) {
    appStore.showStatus(`${t('image.stitchFailed')}：${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
    return null
  } finally {
    processing.value = false
  }
}

/** 拼接参数指纹（变更时触发重新生成）。 */
function stitchFingerprint(): string {
  return [
    stitchFiles.value.join('\0'),
    stitchDirection.value,
    stitchGap.value,
    stitchResize.value,
  ].join('|')
}

let resultFingerprint = ''

// ── 输出目录 ──

async function pickOutputDir() {
  const selected = await withSuppressBlur(() => invoke<string>(CMD.pickDirectory))
  if (selected) config.outputDir = selected
}

function resetOutputDir() {
  config.outputDir = ''
}

// ── 结果操作 ──

async function copyToClipboard() {
  const r = tool.value === 'stitch' ? await ensureStitched() : result.value
  if (!r) return
  try {
    await invoke(CMD.imageCopyToClipboard, { tempPath: r.tempPath })
    appStore.showStatus(t('image.copiedToClipboard'))
  } catch (e) {
    appStore.showStatus(`${t('image.copyFailed')}：${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  }
}

async function saveToFile() {
  const r = tool.value === 'stitch' ? await ensureStitched() : result.value
  if (!r) return
  const sourcePath = tool.value === 'removeBg' ? inputPath.value : stitchFiles.value[0]
  if (!sourcePath) return
  const suffix = tool.value === 'removeBg' ? 'nobg' : 'stitch'
  const outputPath = buildOutputPath(sourcePath, config.outputDir || undefined, suffix)
  try {
    await invoke(CMD.imageSaveResult, { tempPath: r.tempPath, outputPath })
    savedOutputPath.value = outputPath
    appStore.showStatus(t('image.saved'))
  } catch (e) {
    appStore.showStatus(`${t('image.saveFailed')}：${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  }
}

async function revealInFinder() {
  const path = savedOutputPath.value
  if (!path) return
  try {
    await invoke(CMD.revealInFinder, { path })
    hideWindow()
  } catch (e) {
    console.error(e)
  }
}
</script>

<style scoped>
/* 棋盘格背景：仅移除背景预览，直观展示透明区域 */
.checkerboard {
  background-image:
    linear-gradient(45deg, var(--color-fill-4) 25%, transparent 25%),
    linear-gradient(-45deg, var(--color-fill-4) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, var(--color-fill-4) 75%),
    linear-gradient(-45deg, transparent 75%, var(--color-fill-4) 75%);
  background-size: 16px 16px;
  background-position:
    0 0,
    0 8px,
    8px -8px,
    -8px 0;
}

/* 隐藏 number input 的上下箭头 */
:deep(input[type='number']::-webkit-inner-spin-button),
:deep(input[type='number']::-webkit-outer-spin-button) {
  -webkit-appearance: none;
  margin: 0;
}

/* 选中态：outline 不占布局空间，不被图片遮挡 */
.stitch-selected {
  outline: 2px solid var(--color-accent);
  outline-offset: -2px;
}

/* 结果图淡入 */
.result-fade-enter-active {
  transition: opacity 600ms ease-in-out;
}
.result-fade-enter-from {
  opacity: 0;
}
</style>
