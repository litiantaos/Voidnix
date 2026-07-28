<template>
  <div class="flex-col-full-pb">
    <!-- ── 移除背景：预览区 ── -->
    <div v-if="tool === 'removeBg' && previewUrl" p="3" shrink="0" flex="~ col" gap="2">
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
          alt="原图"
        />
        <Transition name="result-fade">
          <img
            v-if="result"
            :src="result.previewDataUrl"
            class="h-full w-full inset-0 absolute object-contain"
            alt="处理结果"
          />
        </Transition>
      </div>
      <div v-if="result" class="flex flex-wrap gap-2">
        <BaseButton icon="i-ri-clipboard-line" @click="copyToClipboard">复制</BaseButton>
        <BaseButton icon="i-ri-save-3-line" @click="saveToFile">保存</BaseButton>
        <BaseButton icon="i-ri-folder-line" @click="revealInFinder">在访达中显示</BaseButton>
      </div>
    </div>

    <!-- ── 拼接：实时预览 = 列表合二为一 ── -->
    <div v-if="tool === 'stitch' && stitchFiles.length" p="3" shrink="0" flex="~ col" gap="2">
      <div
        class="hide-scrollbar border border-divider radius-panel border-solid fill-ctrl"
        relative
        shrink="0"
        :style="[previewStyle, overflowStyle]"
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
            @click="selectedFile = selectedFile === i ? -1 : i"
          >
            <img
              v-if="thumbCache.get(file)"
              :src="thumbCache.get(file)"
              class="block"
              :class="stitchDirection === 'vertical' ? 'w-full h-auto' : 'h-full w-auto max-w-none'"
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

      <!-- 操作条 -->
      <div v-if="stitchFiles.length >= 2" class="flex flex-wrap gap-2">
        <BaseButton :disabled="processing" icon="i-ri-clipboard-line" @click="copyToClipboard"
          >复制</BaseButton
        >
        <BaseButton :disabled="processing" icon="i-ri-save-3-line" @click="saveToFile"
          >保存</BaseButton
        >
        <BaseButton icon="i-ri-folder-line" @click="revealInFinder">在访达中显示</BaseButton>
      </div>

      <!-- 选中条目操作 -->
      <div v-if="selectedFile >= 0" flex="~ wrap" gap="2">
        <BaseButton
          :icon="stitchDirection === 'vertical' ? 'i-ri-arrow-up-line' : 'i-ri-arrow-left-line'"
          :disabled="selectedFile === 0"
          @click="moveUp"
          >{{ stitchDirection === 'vertical' ? '上移' : '左移' }}</BaseButton
        >
        <BaseButton
          :icon="stitchDirection === 'vertical' ? 'i-ri-arrow-down-line' : 'i-ri-arrow-right-line'"
          :disabled="selectedFile === stitchFiles.length - 1"
          @click="moveDown"
          >{{ stitchDirection === 'vertical' ? '下移' : '右移' }}</BaseButton
        >
        <BaseButton variant="danger" icon="i-ri-close-line" @click="removeSelected"
          >移除</BaseButton
        >
      </div>
    </div>

    <BaseSettingsList :items="items" @execute="onExecute">
      <!-- 移除背景：source 行 -->
      <template v-if="tool === 'removeBg'" #trailing-source>
        <div flex gap="2">
          <BaseButton :disabled="processing" @click.stop="pickInput">选择</BaseButton>
          <BaseButton v-if="processing" disabled>处理中…</BaseButton>
          <BaseButton
            v-else-if="inputPath && !result"
            variant="primary"
            :disabled="processing"
            @click.stop="removeBg"
            >移除背景</BaseButton
          >
        </div>
      </template>

      <!-- 拼接：source 行 -->
      <template v-else #trailing-source>
        <div flex gap="2">
          <BaseButton :disabled="processing" @click.stop="pickStitchFiles">添加</BaseButton>
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
          <BaseButton v-if="config.outputDir" @click.stop="resetOutputDir">同目录</BaseButton>
          <BaseButton @click.stop="pickOutputDir">选择</BaseButton>
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
import { isTauri } from '@/utils/tauri'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import type { SettingItem } from '@/types/settings'
import { config } from './config'
import { stitchFiles, tool } from './index'
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

const processing = ref(false)
const result = ref<ImageResult | null>(null)
let savedOutputPath = ''

// ── 移除背景 ──
const inputPath = ref('')
const originalPreview = ref('')

// ── 拼接 ──
const stitchDirection = ref<StitchDirection>('vertical')
const stitchGap = ref(0)
const stitchResize = ref<number>(RESIZE_PRESETS[0])
const selectedFile = ref(-1)
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
  savedOutputPath = ''
  selectedFile.value = -1
  stitchFiles.value = []
  thumbCache.value.clear()
}

// 工具切换：清空当前状态
watch(tool, () => {
  result.value = null
  originalPreview.value = ''
  inputPath.value = ''
  savedOutputPath = ''
  selectedFile.value = -1
})

// 退出扩展（KeepAlive deactivate）：自动清空重置
onDeactivated(() => {
  reset()
  tool.value = 'removeBg'
})

// ── 缩略图加载 ──

watch(
  stitchFiles,
  async (files) => {
    for (const file of files) {
      if (thumbCache.value.has(file)) continue
      try {
        const url = await invoke<string>(CMD.imageReadPreview, { inputPath: file })
        thumbCache.value.set(file, url)
      } catch {
        /* ignore */
      }
    }
  },
  { immediate: true, deep: true },
)

// ── 设置列表 ──

const resizeTitle = computed(() => (stitchDirection.value === 'vertical' ? '宽度' : '高度'))

const resizeOptions = computed(() => RESIZE_PRESETS.map((v) => ({ label: String(v), value: v })))

const removeBgSourceSubtitle = computed(() => {
  if (processing.value) return '正在分割前景…'
  if (!inputPath.value) return '支持 PNG / JPEG / HEIC / WebP 等格式'
  if (result.value) {
    return `${result.value.width}×${result.value.height} · ${formatBytes(result.value.sizeBytes)} · PNG`
  }
  return displayPath(inputPath.value)
})

const items = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []

  if (tool.value === 'removeBg') {
    list.push({
      id: 'source',
      title: inputPath.value ? fileNameFromPath(inputPath.value) : '输入图片',
      subtitle: removeBgSourceSubtitle.value,
      type: 'custom',
      group: '文件',
    })
  } else {
    list.push({
      id: 'source',
      title: '输入图片',
      subtitle: stitchFiles.value.length > 0 ? `${stitchFiles.value.length} 张图片` : undefined,
      type: 'custom',
      group: '文件',
    })
    list.push(
      {
        id: 'direction',
        title: '方向',
        type: 'select',
        value: stitchDirection.value,
        options: [
          { label: '纵向', value: 'vertical' },
          { label: '横向', value: 'horizontal' },
        ],
        update: (v) => {
          stitchDirection.value = v as StitchDirection
        },
        group: '参数',
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
        group: '参数',
      },
      {
        id: 'gap',
        title: '间距',
        type: 'custom',
        group: '参数',
      },
    )
  }

  list.push({
    id: 'outputDir',
    title: '输出目录',
    subtitle: config.outputDir ? displayPath(config.outputDir) : '与源文件相同',
    type: 'custom',
    group: '输出',
  })

  return list
})

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
  savedOutputPath = ''
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
    appStore.showStatus(`处理失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
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
      savedOutputPath = ''
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
    appStore.showStatus(`拼接失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
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
    appStore.showStatus('已复制到剪贴板')
  } catch (e) {
    appStore.showStatus(`复制失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
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
    savedOutputPath = outputPath
    appStore.showStatus('已保存')
  } catch (e) {
    appStore.showStatus(`保存失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
  }
}

async function revealInFinder() {
  const path = savedOutputPath || result.value?.tempPath
  if (!path) return
  try {
    await invoke(CMD.revealInFinder, { path })
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
