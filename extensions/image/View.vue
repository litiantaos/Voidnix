<template>
  <div class="flex-col-full-pb">
    <!-- ── 移除背景：预览区（高度屏高比例，直接渲染无布局动画）── -->
    <div v-if="tool === 'removeBg' && previewUrl" p="x-3 b-3" shrink="0">
      <div
        class="checkerboard border border-divider radius-panel border-solid"
        relative
        shrink="0"
        overflow="hidden"
        :style="previewStyle"
      >
        <img
          v-if="originalPreview"
          :src="originalPreview"
          class="img-fade h-full w-full inset-0 absolute object-contain"
          :class="[{ 'img-loaded': imgLoaded }, { 'img-fade-out': result }]"
          :alt="t('image.original')"
          @load="imgLoaded = true"
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

    <!-- ── 拼接：实时预览 = 列表合二为一（高度屏高比例，直接渲染）── -->
    <div v-if="tool === 'stitch' && imageFiles.length" p="x-3 b-3" shrink="0">
      <div relative shrink="0">
        <!-- z-0 收纳缩略图内联 z-index（首张 = 图片总数）：防拼接 11 张以上首张盖过悬浮操作条 z-10 -->
        <div
          class="hide-scrollbar border border-divider radius-panel border-solid fill-ctrl relative z-0"
          shrink="0"
          :style="[previewStyle, overflowStyle]"
          @click="selectedFile = -1"
        >
          <div
            class="h-full"
            :class="stitchDirection === 'vertical' ? 'flex flex-col' : 'flex flex-row items-center'"
          >
            <div
              v-for="(file, i) in imageFiles"
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
              <span class="text-xs text-white px-0.5 bg-black/40 left-0 top-0 absolute z-1">{{
                i + 1
              }}</span>
            </div>
          </div>
        </div>
        <!-- 选中图悬浮操作条：预览区底部中心（挂 wrapper 不随内容滚动；ui-popup 浮层过渡非布局动画） -->
        <Transition name="ui-popup">
          <div
            v-if="selectedFile >= 0"
            class="p-1 acrylic-bar radius-panel flex gap-1 bottom-2 left-1/2 absolute z-10 -translate-x-1/2"
            @click.stop
          >
            <BaseButton
              :icon="stitchDirection === 'vertical' ? 'i-ri-arrow-up-line' : 'i-ri-arrow-left-line'"
              :title="stitchDirection === 'vertical' ? t('image.moveUp') : t('image.moveLeft')"
              @click.stop="moveUp"
            />
            <BaseButton
              :icon="
                stitchDirection === 'vertical' ? 'i-ri-arrow-down-line' : 'i-ri-arrow-right-line'
              "
              :title="stitchDirection === 'vertical' ? t('image.moveDown') : t('image.moveRight')"
              @click.stop="moveDown"
            />
            <BaseButton
              icon="i-ri-close-line"
              :title="t('image.remove')"
              @click.stop="removeSelected"
            />
          </div>
        </Transition>
      </div>
    </div>

    <BaseSettingsList :items="items" @execute="onExecute">
      <!-- 操作行：有图才显示，回车执行主按钮动作（removeBg 未处理=移除背景 / 其余=保存）。
           removeBg 有结果后移除背景按钮去掉，主按钮换保存 -->
      <template v-if="tool === 'removeBg'" #trailing-operations>
        <div flex gap="2">
          <template v-if="result">
            <BaseButton :disabled="processing" @click.stop="copyToClipboard">{{
              t('image.copy')
            }}</BaseButton>
            <BaseButton variant="primary" :disabled="processing" @click.stop="saveToFile">{{
              t('image.save')
            }}</BaseButton>
          </template>
          <BaseButton v-else variant="primary" :disabled="processing" @click.stop="removeBg">{{
            processing ? t('image.processing') : t('image.removeBg')
          }}</BaseButton>
        </div>
      </template>

      <template v-else #trailing-operations>
        <div flex gap="2">
          <BaseButton :disabled="processing" @click.stop="copyToClipboard">{{
            t('image.copy')
          }}</BaseButton>
          <BaseButton variant="primary" :disabled="processing" @click.stop="saveToFile">{{
            t('image.save')
          }}</BaseButton>
        </div>
      </template>

      <!-- 移除背景：source 行 -->
      <template v-if="tool === 'removeBg'" #trailing-source>
        <BaseButton :disabled="processing" @click.stop="pickInput">{{
          t('image.select')
        }}</BaseButton>
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
          class="bare-number-input w-16"
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
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { currentMonitor } from '@tauri-apps/api/window'
import { CMD } from '@/commands'
import { useAppStore, withSuppressBlur } from '@/stores/app'
import { isTauri } from '@/utils/tauri'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import { t } from '@/runtime/i18n'
import type { SettingItem } from '@/types/settings'
import { config } from './config'
import { pendingInputPath } from './index'
import {
  IMAGE_EXTENSIONS,
  RESIZE_PRESETS,
  bytesFromDataUrl,
  displayPath,
  fileNameFromPath,
  formatBytes,
  imageSizeOf,
  buildOutputPath,
  type ImageResult,
  type Resize,
  type StitchDirection,
  type Tool,
} from './logic'

const appStore = useAppStore()

const processing = ref(false)
const result = ref<ImageResult | null>(null)

/** 当前工具：初始读持久化默认，用户主动切换时写回 config（投递直达只改本地不落盘）。 */
const tool = ref<Tool>(config.defaultTool)

// ── 移除背景 ──
const inputPath = ref('')
const originalPreview = ref('')

// ── 拼接 ──
/** 共享图片集合：两工具单一输入源——removeBg 处理其中「当前张」，拼接消费全列表。 */
const imageFiles = ref<string[]>([])
const stitchDirection = ref<StitchDirection>('vertical')
const stitchGap = ref(0)
const stitchResize = ref<number>(RESIZE_PRESETS[0])
const selectedFile = ref(-1)
/// 拼接缩略图 LRU 缓存：base64 data URL 单张可达数十 KB，无上限时拼接大量图片致内存膨胀
const THUMB_CACHE_MAX = 20
const thumbCache = ref<Map<string, string>>(new Map())
/// 各图字节数（从缩略图 data URL 推算）：数字 Map 不随 LRU 驱逐，总大小汇总不因缓存上限失真
const stitchSizes = ref<Map<string, number>>(new Map())

// ── 屏幕高度：预览区按屏高比例（统一 30%）──
// 进入扩展（onActivated）+ 窗口获焦时刷新：窗口每次 show 定位光标屏，
// 扩展保持激活下跨屏唤起不触发 deactivate/activate 周期，须靠获焦事件换新屏基数
const screenHeight = ref(800)

async function refreshScreenHeight() {
  if (!isTauri) return
  try {
    const m = await currentMonitor()
    if (m) screenHeight.value = m.size.height / m.scaleFactor
  } catch {
    /* ignore */
  }
}

// 获焦回调仅在本扩展激活时刷新（KeepAlive deactivated 期间监听器常驻，跳过无关唤起）
function refreshScreenHeightIfActive() {
  if (appStore.activeExtId === 'image') void refreshScreenHeight()
}

// 预览高度统一单一比例：模式/方向切换预览区零高度跳变（布局动画已移除，等高是防跳手段）
const previewStyle = computed(() => ({ height: `${Math.round(screenHeight.value * 0.3)}px` }))

// 横向仅横向滚动，纵向仅纵向滚动
const overflowStyle = computed(() =>
  stitchDirection.value === 'vertical'
    ? ({ overflowX: 'hidden', overflowY: 'auto' } as const)
    : ({ overflowX: 'auto', overflowY: 'hidden' } as const),
)

const previewUrl = computed(() => (originalPreview.value || result.value?.previewDataUrl) ?? '')

/** 原图就绪淡入：src 变化重置、@load（解码完成）置位——预览框先出后图片平滑淡入，
 * 消除「框突现、图后跳」两步感（元素插入即带终态 class 不会触发 transition，须经状态翻转驱动）。 */
const imgLoaded = ref(false)
watch(originalPreview, () => {
  imgLoaded.value = false
})

/**
 * 每张图的间距/重叠样式 + z-index。
 * gap > 0：正值 margin（间距）；gap < 0：负值 margin（重叠）。
 * 重叠模式（如电影台词拼接）：序号小的在上层，首张图完整显示，
 * 后续图上半部分被前一张覆盖，仅露出底部台词。Rust 端逆序绘制对齐。
 */
function itemStyle(index: number): Record<string, string> {
  const g = stitchGap.value
  const isVertical = stitchDirection.value === 'vertical'
  const last = index === imageFiles.value.length - 1
  const style: Record<string, string> = {
    zIndex: String(imageFiles.value.length - index),
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
  inputMeta.value = ''
  selectedFile.value = -1
  imageFiles.value = []
  thumbCache.value.clear()
  stitchSizes.value.clear()
}

// 工具切换：清操作结果（结果属于特定工具），共享集合与 removeBg 当前张保留
watch(tool, () => {
  result.value = null
  if (tool.value === 'removeBg') {
    // 当前张解析：仍在集合则保持；否则取拼接选中项、首张兜底（selectedFile 清除前取值）
    const files = imageFiles.value
    const target =
      inputPath.value && files.includes(inputPath.value)
        ? inputPath.value
        : files[selectedFile.value >= 0 ? selectedFile.value : 0]
    selectedFile.value = -1
    if (target && target !== inputPath.value) {
      void setInput(target)
    } else if (!target) {
      inputPath.value = ''
      originalPreview.value = ''
      inputMeta.value = ''
    }
  }
})

// 退出扩展（KeepAlive deactivate）：自动清空重置（工具选择保留，同 video 模式语义）
onDeactivated(() => {
  reset()
})

// 屏高刷新：进入扩展时触发（KeepAlive 树内首挂载即触发 activated）；跨屏唤起经
// window-focused 补刷（获焦回调自带激活判断，deactivated 期间跳过）
onActivated(refreshScreenHeight)
onMounted(() => window.addEventListener('window-focused', refreshScreenHeightIfActive))
onUnmounted(() => window.removeEventListener('window-focused', refreshScreenHeightIfActive))

// 跨扩展进入的投递消费见下方 setInput 之后的 watch（依赖 inputMeta / previewToken 等声明，
// immediate 回调会在注册点同步执行，须置于全部依赖声明之后）。

// ── 缩略图加载 ──

watch(
  imageFiles,
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
        const bytes = bytesFromDataUrl(url)
        if (bytes !== null) stitchSizes.value.set(file, bytes)
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
  return inputMeta.value || displayPath(inputPath.value)
})

/** 拼接总大小副标题：按当前列表已推算项累计（随缩略图加载渐进），全未推算返回 undefined。 */
const stitchTotalSubtitle = computed(() => {
  let total = 0
  let known = false
  for (const file of imageFiles.value) {
    const bytes = stitchSizes.value.get(file)
    if (bytes === undefined) continue
    total += bytes
    known = true
  }
  return known ? t('image.totalSize', { size: formatBytes(total) }) : undefined
})

const items = computed<SettingItem[]>(() => {
  const list: SettingItem[] = []

  // 模式选择：列表第一项（通用组——两工具共享的全局设置，区别于模式特有参数）
  list.push({
    id: 'tool',
    title: t('image.mode'),
    type: 'select',
    value: tool.value,
    options: [
      { label: t('image.removeBg'), value: 'removeBg' },
      { label: t('image.stitch'), value: 'stitch' },
    ],
    update: (v) => {
      tool.value = v as Tool
      config.defaultTool = v as Tool
    },
    group: t('image.group.common'),
  })

  // 操作：通用组第二项，有图才显示（拼接需两张起——按钮零数量禁用，可操作性与显示同步）；
  // 回车执行主按钮动作（removeBg 未处理=移除背景 / 其余=保存），按钮经 trailing 插槽
  // （removeBg 的复制/保存等有结果后再显示）
  const hasImage = tool.value === 'removeBg' ? !!inputPath.value : imageFiles.value.length >= 2
  if (hasImage) {
    list.push({
      id: 'operations',
      title: t('image.operations'),
      type: 'custom',
      group: t('image.group.common'),
    })
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
      title:
        imageFiles.value.length === 0
          ? t('image.inputImage')
          : imageFiles.value.length === 1
            ? fileNameFromPath(imageFiles.value[0])
            : t('image.imageCount', { count: imageFiles.value.length }),
      subtitle:
        imageFiles.value.length === 0 ? t('image.formatsHintMulti') : stitchTotalSubtitle.value,
      type: 'custom',
      group: t('image.group.file'),
    })
  }

  if (tool.value === 'stitch') {
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

function onExecute(item: SettingItem) {
  if (item.id === 'operations') {
    // 回车 = 主按钮动作：removeBg 未处理时移除背景，其余（已处理 / 拼接）保存
    if (tool.value === 'removeBg' && !result.value) void removeBg()
    else void saveToFile()
    return
  }
  if (item.id === 'outputDir') {
    void pickOutputDir()
    return
  }
  if (item.id === 'source') {
    if (tool.value === 'removeBg') {
      if (!processing.value) void pickInput()
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

/** 输入图元数据（`宽×高 · 大小`）：从预览 data URL 前端推算，零 probe 命令。 */
const inputMeta = ref('')

/// 预览请求令牌：快速换图时旧请求后到（大图解码慢）直接丢弃，防旧图元数据错配覆盖到新文件
let previewToken = 0

async function loadPreview(path: string) {
  inputMeta.value = ''
  const token = ++previewToken
  try {
    const url = await invoke<string>(CMD.imageReadPreview, { inputPath: path })
    if (token !== previewToken) return
    originalPreview.value = url
    const bytes = bytesFromDataUrl(url)
    const dim = await imageSizeOf(url)
    if (token !== previewToken) return
    inputMeta.value = [dim, bytes !== null ? formatBytes(bytes) : ''].filter(Boolean).join(' · ')
  } catch {
    if (token !== previewToken) return
    originalPreview.value = ''
    inputMeta.value = ''
  }
}

async function pickInput() {
  await withSuppressBlur(async () => {
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: false,
      allowedExtensions: [...IMAGE_EXTENSIONS],
    })
    if (paths[0]) {
      addImage(paths[0])
      await setInput(paths[0])
    }
  })
}

/** 图片进共享集合（去重，追加末尾）：removeBg 单选 / 跨扩展投递与拼接添加同源。 */
function addImage(path: string) {
  if (!imageFiles.value.includes(path)) imageFiles.value.push(path)
}

async function setInput(path: string) {
  inputPath.value = path
  result.value = null
  await loadPreview(path)
}

// 跨扩展进入：finder-ext 等经同页 CustomEvent 投递的待处理图片路径，写入即加载。
// 投递与 setActiveExtension 同步发生：缓存态（KeepAlive 存活）watch 在同一 flush 内
// 先于渲染执行、inputPath 同步置位——跳转首帧列表即含 operations 行（形状定型）；
// LRU 驱逐重挂载场景经 immediate 在 mount 时消费（watch 注册晚于投递写入，immediate 补齐）。
// 单张投递按抠图意图直达 removeBg（只改本地不落盘默认）。
// 位置约束：immediate 回调在注册点同步执行，须置于全部依赖（inputMeta / previewToken /
// addImage / setInput）声明之后。
watch(
  pendingInputPath,
  (path) => {
    if (!path) return
    pendingInputPath.value = ''
    tool.value = 'removeBg'
    addImage(path)
    void setInput(path)
  },
  { immediate: true },
)

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
      const seen = new Set(imageFiles.value)
      const fresh = paths.filter((p) => !seen.has(p))
      if (imageFiles.value.length === 0) {
        // 首次添加：按文件名升序
        imageFiles.value = sortByFileName(fresh)
      } else {
        // 后续添加：追加到末尾
        imageFiles.value = [...imageFiles.value, ...fresh]
      }
    }
  })
}

function moveUp() {
  const i = selectedFile.value
  if (i <= 0) return
  const list = imageFiles.value
  ;[list[i - 1], list[i]] = [list[i], list[i - 1]]
  selectedFile.value = i - 1
}

function moveDown() {
  const i = selectedFile.value
  const list = imageFiles.value
  if (i >= list.length - 1) return
  ;[list[i + 1], list[i]] = [list[i], list[i + 1]]
  selectedFile.value = i + 1
}

function removeSelected() {
  const i = selectedFile.value
  if (i < 0) return
  const removed = imageFiles.value[i]
  imageFiles.value.splice(i, 1)
  selectedFile.value = -1
  if (removed === inputPath.value) {
    // removeBg 当前张被移除：清其侧状态（切回时经 watch(tool) 兜底取集合剩余首张）
    inputPath.value = ''
    originalPreview.value = ''
    inputMeta.value = ''
    result.value = null
  }
}

/** 生成拼接结果（复制/保存时惰性调用，文件或参数变更后自动重新生成）。 */
async function ensureStitched(): Promise<ImageResult | null> {
  // processing 守卫：回车路径可绕过按钮 disabled，重复触发会打到 Rust BUSY 锁
  if (processing.value) return null
  if (imageFiles.value.length < 2) return null
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
      inputPaths: imageFiles.value,
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
    imageFiles.value.join('\0'),
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
  const sourcePath = tool.value === 'removeBg' ? inputPath.value : imageFiles.value[0]
  if (!sourcePath) return
  const suffix = tool.value === 'removeBg' ? 'nobg' : 'stitch'
  const outputPath = buildOutputPath(sourcePath, config.outputDir || undefined, suffix)
  try {
    await invoke(CMD.imageSaveResult, { tempPath: r.tempPath, outputPath })
    appStore.showStatus(t('image.saved'))
  } catch (e) {
    appStore.showStatus(`${t('image.saveFailed')}：${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
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

/* 选中态：outline 不占布局空间，不被图片遮挡 */
.stitch-selected {
  outline: 2px solid var(--color-accent);
  outline-offset: -2px;
}

/* 原图/结果图交叉淡入：两侧必须同值成对（600ms 为定制交叉时长，无基元档）。
 * 原图起步 opacity 0，@load 解码完成经 img-loaded 翻转淡入（首次/换图不突现）；
 * img-fade-out 为结果淡出语义，须胜过 img-loaded（!important）。 */
.result-fade-enter-active,
.img-fade {
  transition: opacity 600ms ease-in-out;
}
.result-fade-enter-from {
  opacity: 0;
}
.img-fade {
  opacity: 0;
}
.img-loaded {
  opacity: 1;
}
.img-fade-out {
  opacity: 0 !important;
}
</style>
