<template>
  <div
    ref="rootEl"
    class="select-none inset-0 fixed z-[9999] overflow-hidden"
    :style="{ cursor: cursorStyle }"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @keydown="onKeyDown"
    tabindex="0"
  >
    <!-- 截图背景：由原生 CALayer 直贴 CGImage，零编码零拷贝，此处不再渲染 -->

    <!-- 遮罩层（选区外置灰，选区内透明） -->
    <template v-if="hasSelection">
      <div class="bg-black/45 pointer-events-none fixed" :style="maskTop" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskBottom" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskLeft" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskRight" />
    </template>
    <!-- 窗口高亮时：挖洞遮罩（内部全透明）；否则全屏遮罩 -->
    <template v-else-if="!hasSelection && phase === 'select' && hoverWindow">
      <div class="bg-black/45 pointer-events-none fixed" :style="hoverMaskTop" />
      <div class="bg-black/45 pointer-events-none fixed" :style="hoverMaskBottom" />
      <div class="bg-black/45 pointer-events-none fixed" :style="hoverMaskLeft" />
      <div class="bg-black/45 pointer-events-none fixed" :style="hoverMaskRight" />
    </template>
    <div v-else class="bg-black/45 pointer-events-none inset-0 fixed" />

    <!-- 十字线（未确定选区时，始终显示） -->
    <template v-if="!hasSelection && phase === 'select'">
      <div class="bg-accent/80 pointer-events-none absolute" style="left: 0; right: 0; height: 1px; top: var(--cross-y)" />
      <div class="bg-accent/80 pointer-events-none absolute" style="top: 0; bottom: 0; width: 1px; left: var(--cross-x)" />
    </template>

    <!-- 放大镜小窗（select 阶段，未拖动时显示） -->
    <div
      v-if="phase === 'select' && !isDragging && !pendingDrag"
      class="border border-black/20 rounded-lg pointer-events-none shadow-xl absolute z-50 overflow-hidden"
      :style="magnifierStyle"
    >
      <!-- 取色画布区：含 canvas 与十字准星，准星严格限制在画布范围内 -->
      <div class="relative" :style="{ width: MAGNIFIER_SIZE + 'px', height: MAGNIFIER_SIZE + 'px' }">
        <canvas ref="magnifierCanvas" :width="MAGNIFIER_SIZE * dpr" :height="MAGNIFIER_SIZE * dpr" :style="{ width: MAGNIFIER_SIZE + 'px', height: MAGNIFIER_SIZE + 'px' }" class="block" />
        <!-- 十字准星：与外部十字线同色 -->
        <div class="pointer-events-none inset-0 absolute">
          <div class="bg-accent/80 absolute" :style="{ left: 0, right: 0, height: '1px', top: `${MAGNIFIER_SIZE / 2}px` }" />
          <div class="bg-accent/80 absolute" :style="{ top: 0, bottom: 0, width: '1px', left: `${MAGNIFIER_SIZE / 2}px` }" />
        </div>
      </div>
      <!-- 色值标签：白底黑字，左侧方块直观显示当前色 -->
      <div class="text-xs text-black font-mono h-6 bg-white flex gap-2 select-none justify-center items-center border-t border-black/20">
        <div class="border border-black/20 h-3 w-3" :style="{ background: pickedColor }"></div>
        <div>{{ pickedColor }}</div>
      </div>
    </div>

    <!-- 智能识别：光标下窗口高亮预览（select 阶段且未拖动时显示） -->
    <!-- 窗口高亮边框与标签（遮罩已在上方处理） -->
    <template v-if="!hasSelection && phase === 'select' && hoverWindow">
      <div
        class="border border-accent pointer-events-none absolute"
        :style="hoverWindowStyle"
      >
        <!-- 尺寸标签：与手动选区样式一致 -->
        <div
          class="text-xs text-tx-primary px-1.5 py-0.5 rounded bg-surface pointer-events-none select-none shadow absolute"
          :style="hoverSizeStyle"
        >{{ Math.round(hoverWindow.w) }}×{{ Math.round(hoverWindow.h) }}</div>
      </div>
    </template>

    <!-- 选区边框 + 控制点 + 尺寸标签 -->
    <template v-if="hasSelection">
      <div
        class="border border-accent pointer-events-none absolute"
        :style="selectionStyle"
      >
        <!-- 选区尺寸标签 -->
        <div
          class="text-xs text-tx-primary px-1.5 py-0.5 rounded bg-surface pointer-events-none select-none shadow absolute"
          :style="selSizeStyle"
        >{{ Math.round(sel.w) }}×{{ Math.round(sel.h) }}</div>
        <!-- 8个控制点 -->
        <div v-for="h in handles" :key="h.id"
          class="border border-accent rounded-sm bg-white h-2 w-2 pointer-events-auto absolute"
          :style="h.style"
          :data-handle="h.id"
          @mousedown.stop="startResize(h.id, $event)"
        />
      </div>
    </template>

    <!-- 标注 canvas -->
    <canvas
      v-if="hasSelection && phase === 'annotate'"
      ref="annotateCanvas"
      class="pointer-events-none absolute"
      :style="{ left: `${sel.x}px`, top: `${sel.y}px`, width: `${sel.w}px`, height: `${sel.h}px` }"
      :width="sel.w * dpr"
      :height="sel.h * dpr"
    />

    <!-- 文字输入框（内联，替代 prompt） -->
    <textarea
      v-if="textInput.visible"
      ref="textInputEl"
      v-model="textInput.value"
      class="text-sm leading-tight outline-none border border-accent bg-transparent resize-none absolute z-50"
      :style="textInputStyle"
      @keydown.enter.exact.prevent="commitText"
      @keydown.escape="cancelText"
      @mousedown.stop
    />

    <!-- 标注调色板 -->
    <AnnotationPalette
      v-if="hasSelection"
      :sel="sel"
      :active-tool="activeTool"
      :color="annotColor"
      :line-width="annotLineWidth"
      :screen-height="screenH"
      :screen-width="screenW"
      @tool="setTool"
      @color="annotColor = $event"
      @line-width="annotLineWidth = $event"
      @ocr="doOcr"
      @copy="doCopy"
      @save="doSave"
      @cancel="doCancel"
    />

    <!-- OCR 结果浮层 -->
    <div
      v-if="ocrText !== null"
      class="text-sm text-tx-primary p-3 border border-black/10 rounded-lg bg-surface max-w-sm select-text whitespace-pre-wrap shadow-xl absolute z-50"
      :style="ocrPopupStyle"
    >
      <div class="mb-2 flex items-center justify-between">
        <span class="text-xs text-tx-muted">OCR 识别结果</span>
        <div class="flex gap-1">
          <button class="text-xs px-2 py-0.5 rounded" @click="copyOcr">复制</button>
          <button class="text-xs px-2 py-0.5 rounded" @click="ocrText = null">✕</button>
        </div>
      </div>
      <div v-if="ocrLoading" class="text-tx-muted">识别中…</div>
      <div v-else>{{ ocrText || '未识别到文字' }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import AnnotationPalette from './AnnotationPalette.vue'
import { useSettingsStore } from '@/stores/settings'

// 文字输入内联状态
const textInputEl = ref<HTMLTextAreaElement>()
const textInput = ref({ visible: false, value: '', x: 0, y: 0, canvasX: 0, canvasY: 0 })
const textInputStyle = computed(() => ({
  left: `${textInput.value.x}px`,
  top: `${textInput.value.y}px`,
  color: annotColor.value,
  fontSize: `${Math.max(14, annotLineWidth.value * 6)}px`,
  minWidth: '80px',
  minHeight: '24px',
}))

function openTextInput(screenX: number, screenY: number) {
  textInput.value = {
    visible: true,
    value: '',
    x: screenX,
    y: screenY,
    canvasX: screenX - sel.value.x,
    canvasY: screenY - sel.value.y,
  }
  nextTick(() => textInputEl.value?.focus())
}

function commitText() {
  // 防止重复提交（blur 和 Enter 可能同时触发）
  if (!textInput.value.visible) return
  const t = textInput.value.value.trim()
  textInput.value.visible = false
  textInput.value.value = ''
  if (t) {
    shapes.value.push({
      type: 'text',
      x1: textInput.value.canvasX,
      y1: textInput.value.canvasY,
      x2: textInput.value.canvasX,
      y2: textInput.value.canvasY,
      color: annotColor.value,
      lineWidth: annotLineWidth.value,
      text: t,
    })
    redraw()
  }
  nextTick(() => rootEl.value?.focus())
}

function cancelText() {
  textInput.value.visible = false
  textInput.value.value = ''
  nextTick(() => rootEl.value?.focus())
}

interface WindowRect { x: number; y: number; w: number; h: number; owner: string }
interface ScreenshotData { data_url: string; width: number; height: number; scale: number; mouse_x: number; mouse_y: number; windows: WindowRect[] }
interface Sel { x: number; y: number; w: number; h: number }

const props = defineProps<{ initialScreenshot: ScreenshotData }>()
const emit = defineEmits<{ (e: 'close'): void }>()

const rootEl = ref<HTMLElement>()
const annotateCanvas = ref<HTMLCanvasElement>()

const screenW = ref(props.initialScreenshot.width)
const screenH = ref(props.initialScreenshot.height)
const dpr = ref(props.initialScreenshot.scale)
const windows = ref<WindowRect[]>(props.initialScreenshot.windows ?? [])

// 背景图 URL 和 bgImage 已移除：背景由原生 CALayer 直贴 CGImage（零编码零拷贝）。
// 放大镜取色用的 bgImage 改为异步加载 picker JPEG（capture 后台线程生成）。
const bgImage = ref<HTMLImageElement | null>(null)
;(async function loadPickerImage() {
  const pickerPath = props.initialScreenshot.data_url
  if (!pickerPath) return
  try {
    const { convertFileSrc } = await import('@tauri-apps/api/core')
    const url = convertFileSrc(pickerPath) + `?t=${Date.now()}`
    // 轮询等待文件就绪（后台线程异步生成，通常 50-100ms 内完成）
    for (let i = 0; i < 20; i++) {
      try {
        const resp = await fetch(url)
        if (resp.ok) {
          const blob = await resp.blob()
          const objectUrl = URL.createObjectURL(blob)
          const img = new Image()
          img.onload = () => { bgImage.value = img }
          img.src = objectUrl
          return
        }
      } catch { /* 文件未就绪，继续等待 */ }
      await new Promise(r => setTimeout(r, 50))
    }
  } catch { /* 放大镜不可用，不影响主流程 */ }
})()

// 选区状态
type Phase = 'select' | 'annotate'
const phase = ref<Phase>('select')
const sel = ref<Sel>({ x: 0, y: 0, w: 0, h: 0 })
const dragStart = ref({ x: 0, y: 0 })
const isDragging = ref(false)
// pendingDrag：mousedown 后未达拖动阈值前的临时状态，达阈值才转为真正的 isDragging。
// 单击不拖动时用于触发"采用 hoverWindow"逻辑
const pendingDrag = ref(false)
const DRAG_THRESHOLD = 4

// 智能识别：当前光标下最顶层的窗口（select 阶段且未拖动时计算）
const hoverWindow = ref<WindowRect | null>(null)

function findWindowAt(cx: number, cy: number): WindowRect | null {
  // windows 已按 z-order 从顶到底排列，第一个命中即为顶层
  for (const w of windows.value) {
    if (cx >= w.x && cx <= w.x + w.w && cy >= w.y && cy <= w.y + w.h) {
      return w
    }
  }
  return null
}
const resizeHandle = ref<string | null>(null)
const resizeStart = ref({ x: 0, y: 0, sel: { x: 0, y: 0, w: 0, h: 0 } })

const hasSelection = computed(() => sel.value.w > 4 && sel.value.h > 4)

// 标注工具
type Tool = 'rect' | 'line' | 'arrow' | 'text' | 'blur' | null
const activeTool = ref<Tool>(null)
const annotColor = ref('#ff3b30')
const annotLineWidth = ref(2)

// OCR
const ocrText = ref<string | null>(null)
const ocrLoading = ref(false)

// 标注绘制状态
const isDrawing = ref(false)
const drawStart = ref({ x: 0, y: 0 })
const shapes = ref<Shape[]>([])
const currentShape = ref<Shape | null>(null)

interface Shape {
  type: Tool
  x1: number; y1: number; x2: number; y2: number
  color: string; lineWidth: number
  text?: string
}

// ── 选区样式 ──────────────────────────────────────────────
const selectionStyle = computed(() => ({
  left: `${sel.value.x}px`,
  top: `${sel.value.y}px`,
  width: `${sel.value.w}px`,
  height: `${sel.value.h}px`,
}))

const hoverWindowStyle = computed(() => {
  const w = hoverWindow.value
  if (!w) return {}
  return {
    left: `${w.x}px`,
    top: `${w.y}px`,
    width: `${w.w}px`,
    height: `${w.h}px`,
  }
})

// 窗口高亮遮罩挖洞（与选区遮罩逻辑相同）
const hoverMaskTop = computed(() => {
  const w = hoverWindow.value
  if (!w) return {}
  return { left: 0, top: 0, right: 0, height: `${w.y}px` }
})
const hoverMaskBottom = computed(() => {
  const w = hoverWindow.value
  if (!w) return {}
  return { left: 0, bottom: 0, right: 0, top: `${w.y + w.h}px` }
})
const hoverMaskLeft = computed(() => {
  const w = hoverWindow.value
  if (!w) return {}
  return { left: 0, top: `${w.y}px`, width: `${w.x}px`, height: `${w.h}px` }
})
const hoverMaskRight = computed(() => {
  const w = hoverWindow.value
  if (!w) return {}
  return { right: 0, top: `${w.y}px`, left: `${w.x + w.w}px`, height: `${w.h}px` }
})

const maskTop = computed(() => ({
  left: 0, top: 0, right: 0, height: `${sel.value.y}px`,
}))
const maskBottom = computed(() => ({
  left: 0, bottom: 0, right: 0,
  top: `${sel.value.y + sel.value.h}px`,
}))
const maskLeft = computed(() => ({
  left: 0, top: `${sel.value.y}px`,
  width: `${sel.value.x}px`, height: `${sel.value.h}px`,
}))
const maskRight = computed(() => ({
  right: 0, top: `${sel.value.y}px`,
  left: `${sel.value.x + sel.value.w}px`, height: `${sel.value.h}px`,
}))

// 标注调色板位置预测：与 AnnotationPalette 内部 style 计算保持一致
// 'below'：选区下方外侧 / 'above'：选区上方外侧 / 'inside'：选区内底部
const PALETTE_H = 44
const PALETTE_GAP = 8
const palettePosition = computed<'below' | 'above' | 'inside'>(() => {
  const { y, h } = sel.value
  if (y + h + PALETTE_GAP + PALETTE_H <= screenH.value) return 'below'
  if (y - PALETTE_H - PALETTE_GAP >= 0) return 'above'
  return 'inside'
})

// 选区尺寸标签位置：默认显示在选区左上角外侧，空间不足时显示在内侧
// 当调色板也在选区上方外侧时，尺寸标签强制显示在内侧，避免与之重叠
const selSizeStyle = computed(() => {
  const { y } = sel.value
  if (y >= 22 && palettePosition.value !== 'above') {
    return { top: '-22px', left: '0px' }
  }
  return { top: '4px', left: '4px' }
})

// 窗口高亮尺寸标签位置：与 selSizeStyle 逻辑一致
const hoverSizeStyle = computed(() => {
  const w = hoverWindow.value
  if (!w) return {}
  if (w.y >= 22) {
    return { top: '-22px', left: '0px' }
  }
  return { top: '4px', left: '4px' }
})

const handles = computed(() => {
  const { w, h } = sel.value
  const half = -4
  return [
    { id: 'nw', style: { left: `${half}px`, top: `${half}px`, cursor: 'nw-resize' } },
    { id: 'n',  style: { left: `${w/2 + half}px`, top: `${half}px`, cursor: 'n-resize' } },
    { id: 'ne', style: { right: `${half}px`, top: `${half}px`, cursor: 'ne-resize' } },
    { id: 'w',  style: { left: `${half}px`, top: `${h/2 + half}px`, cursor: 'w-resize' } },
    { id: 'e',  style: { right: `${half}px`, top: `${h/2 + half}px`, cursor: 'e-resize' } },
    { id: 'sw', style: { left: `${half}px`, bottom: `${half}px`, cursor: 'sw-resize' } },
    { id: 's',  style: { left: `${w/2 + half}px`, bottom: `${half}px`, cursor: 's-resize' } },
    { id: 'se', style: { right: `${half}px`, bottom: `${half}px`, cursor: 'se-resize' } },
  ]
})

const cursorStyle = computed(() => {
  if (resizeHandle.value) return getCursorForHandle(resizeHandle.value)
  return 'default'
})

function getCursorForHandle(h: string) {
  const map: Record<string, string> = {
    nw: 'nw-resize', n: 'n-resize', ne: 'ne-resize',
    w: 'w-resize', e: 'e-resize',
    sw: 'sw-resize', s: 's-resize', se: 'se-resize',
  }
  return map[h] || 'default'
}

const ocrPopupStyle = computed(() => {
  const { x, y, h } = sel.value
  const top = y + h + 8
  const left = Math.min(x, screenW.value - 320)
  return { top: `${top}px`, left: `${left}px` }
})

// ── 放大镜 ────────────────────────────────────────────────
const MAGNIFIER_SIZE = 120   // 放大镜 canvas 尺寸（逻辑像素）
const MAGNIFIER_ZOOM = 4     // 放大倍率
const MAGNIFIER_OFFSET = 20  // 距光标的偏移

const magnifierCanvas = ref<HTMLCanvasElement>()
const pickedColor = ref('#000000')

// 放大镜位置：优先显示在光标右下，空间不足时翻转
const crossX = ref(props.initialScreenshot.mouse_x)
const crossY = ref(props.initialScreenshot.mouse_y)

const magnifierStyle = computed(() => {
  const totalH = MAGNIFIER_SIZE + 20 // canvas + 色值标签
  // 默认显示在光标左下角
  let left = crossX.value - MAGNIFIER_SIZE - MAGNIFIER_OFFSET
  let top = crossY.value + MAGNIFIER_OFFSET
  // 左边放不下时翻到右边
  if (left < 0) left = crossX.value + MAGNIFIER_OFFSET
  // 下边放不下时翻到上边
  if (top + totalH > screenH.value) top = crossY.value - totalH - MAGNIFIER_OFFSET
  return { left: `${left}px`, top: `${top}px`, width: `${MAGNIFIER_SIZE}px` }
})

function updateMagnifier(cx: number, cy: number) {
  const canvas = magnifierCanvas.value
  if (!canvas || !bgImage.value) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const sc = dpr.value
  // canvas 物理尺寸 = MAGNIFIER_SIZE * sc
  const canvasSize = MAGNIFIER_SIZE * sc
  // 放大镜覆盖的逻辑像素范围（在屏幕上）
  const half = MAGNIFIER_SIZE / MAGNIFIER_ZOOM / 2
  // bgImage 是物理像素图，source 坐标直接用物理像素
  const sx = (cx - half) * sc
  const sy = (cy - half) * sc
  const sw = (MAGNIFIER_SIZE / MAGNIFIER_ZOOM) * sc
  const sh = (MAGNIFIER_SIZE / MAGNIFIER_ZOOM) * sc

  ctx.clearRect(0, 0, canvasSize, canvasSize)
  ctx.imageSmoothingEnabled = false
  ctx.drawImage(bgImage.value, sx, sy, sw, sh, 0, 0, canvasSize, canvasSize)

  // 取中心像素颜色（物理坐标）
  const px = ctx.getImageData(Math.floor(canvasSize / 2), Math.floor(canvasSize / 2), 1, 1).data
  const hex = '#' + [px[0], px[1], px[2]].map(v => v.toString(16).padStart(2, '0')).join('')
  pickedColor.value = hex
}

// ── 鼠标事件 ──────────────────────────────────────────────
function onMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  const { clientX: cx, clientY: cy } = e

  // 如果文字输入框正在显示，先提交再处理新点击
  if (textInput.value.visible) {
    commitText()
    // 如果还是文字工具，继续在新位置打开输入框
    if (activeTool.value === 'text') {
      openTextInput(cx, cy)
    }
    return
  }

  if (phase.value === 'select') {
    // 进入待拖动状态：移动距离 > DRAG_THRESHOLD 才转为真正的拖选
    pendingDrag.value = true
    dragStart.value = { x: cx, y: cy }
    return
  }

  // annotate phase
  if (!activeTool.value) {
    // 拖动选区
    if (isInsideSel(cx, cy)) {
      isDragging.value = true
      dragStart.value = { x: cx - sel.value.x, y: cy - sel.value.y }
    }
    return
  }

  if (activeTool.value === 'text') {
    openTextInput(cx, cy)
    return
  }

  isDrawing.value = true
  drawStart.value = { x: cx - sel.value.x, y: cy - sel.value.y }
  currentShape.value = {
    type: activeTool.value,
    x1: drawStart.value.x, y1: drawStart.value.y,
    x2: drawStart.value.x, y2: drawStart.value.y,
    color: annotColor.value, lineWidth: annotLineWidth.value,
  }
}

function onMouseMove(e: MouseEvent) {
  const { clientX: cx, clientY: cy } = e
  // 直接在根容器上更新十字线位置，绕过 Vue 响应式批处理延迟
  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${cx}px`)
    rootEl.value.style.setProperty('--cross-y', `${cy}px`)
  }
  // 更新放大镜
  crossX.value = cx
  crossY.value = cy
  if (phase.value === 'select' && !isDragging.value && !pendingDrag.value) {
    updateMagnifier(cx, cy)
  }

  if (phase.value === 'select' && pendingDrag.value) {
    // 距离超过阈值，从待拖动转为真正拖选；否则继续显示窗口高亮
    const dx = cx - dragStart.value.x
    const dy = cy - dragStart.value.y
    if (Math.abs(dx) >= DRAG_THRESHOLD || Math.abs(dy) >= DRAG_THRESHOLD) {
      pendingDrag.value = false
      isDragging.value = true
      hoverWindow.value = null
      sel.value = { x: dragStart.value.x, y: dragStart.value.y, w: 0, h: 0 }
    }
  }

  if (phase.value === 'select' && isDragging.value) {
    // 选框坐标限制在屏幕范围内
    const clampedCx = Math.max(0, Math.min(cx, screenW.value))
    const clampedCy = Math.max(0, Math.min(cy, screenH.value))
    let w = Math.abs(clampedCx - dragStart.value.x)
    let h = Math.abs(clampedCy - dragStart.value.y)
    // Shift：保持正方形
    if (e.shiftKey) {
      const side = Math.min(w, h)
      w = side; h = side
    }
    const x = clampedCx >= dragStart.value.x ? dragStart.value.x : dragStart.value.x - w
    const y = clampedCy >= dragStart.value.y ? dragStart.value.y : dragStart.value.y - h
    sel.value = { x, y, w, h }
    return
  }

  // select 阶段且未在拖动：实时识别光标下的窗口
  if (phase.value === 'select' && !isDragging.value && !pendingDrag.value) {
    hoverWindow.value = findWindowAt(cx, cy)
  }

  if (resizeHandle.value) {
    applyResize(cx, cy)
    return
  }

  if (phase.value === 'annotate' && isDragging.value && !activeTool.value) {
    // 拖动选区时限制在屏幕范围内
    const newX = cx - dragStart.value.x
    const newY = cy - dragStart.value.y
    sel.value.x = Math.max(0, Math.min(newX, screenW.value - sel.value.w))
    sel.value.y = Math.max(0, Math.min(newY, screenH.value - sel.value.h))
    return
  }

  if (isDrawing.value && currentShape.value) {
    let x2 = cx - sel.value.x
    let y2 = cy - sel.value.y
    // Shift 约束：直线和箭头保持水平或垂直
    if (e.shiftKey && (currentShape.value.type === 'line' || currentShape.value.type === 'arrow')) {
      const dx = Math.abs(x2 - currentShape.value.x1)
      const dy = Math.abs(y2 - currentShape.value.y1)
      if (dx >= dy) {
        y2 = currentShape.value.y1
      } else {
        x2 = currentShape.value.x1
      }
    }
    currentShape.value.x2 = x2
    currentShape.value.y2 = y2
    redraw(currentShape.value)
  }
}

function onMouseUp(_e: MouseEvent) {
  // select 阶段：单击未拖动 → 采用 hoverWindow 作为选区
  if (phase.value === 'select' && pendingDrag.value) {
    pendingDrag.value = false
    if (hoverWindow.value) {
      sel.value = {
        x: hoverWindow.value.x,
        y: hoverWindow.value.y,
        w: hoverWindow.value.w,
        h: hoverWindow.value.h,
      }
      hoverWindow.value = null
      phase.value = 'annotate'
      nextTick(() => rootEl.value?.focus())
    }
    return
  }

  if (phase.value === 'select' && isDragging.value) {
    isDragging.value = false
    if (hasSelection.value) {
      phase.value = 'annotate'
      nextTick(() => rootEl.value?.focus())
    }
    return
  }

  if (resizeHandle.value) {
    resizeHandle.value = null
    return
  }

  isDragging.value = false

  if (isDrawing.value && currentShape.value) {
    shapes.value.push({ ...currentShape.value })
    currentShape.value = null
    isDrawing.value = false
    redraw()
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') doCancel()
  if (e.key === 'Enter' && hasSelection.value) doCopy()
  if ((e.metaKey || e.ctrlKey) && e.key === 'z') {
    shapes.value.pop()
    redraw()
  }
  // F 键：框选全屏
  if (e.key === 'f' || e.key === 'F') {
    sel.value = { x: 0, y: 0, w: screenW.value, h: screenH.value }
    phase.value = 'annotate'
    hoverWindow.value = null
    isDragging.value = false
    pendingDrag.value = false
    nextTick(() => rootEl.value?.focus())
  }
  // C 键：拷贝放大镜色值（select 阶段）
  if ((e.key === 'c' || e.key === 'C') && phase.value === 'select' && !e.metaKey && !e.ctrlKey) {
    writeText(pickedColor.value).catch(() => {})
  }
}

// ── 调整选区大小 ──────────────────────────────────────────
function startResize(handle: string, e: MouseEvent) {
  resizeHandle.value = handle
  resizeStart.value = {
    x: e.clientX, y: e.clientY,
    sel: { ...sel.value },
  }
  e.preventDefault()
}

function applyResize(cx: number, cy: number) {
  const dx = cx - resizeStart.value.x
  const dy = cy - resizeStart.value.y
  const s = { ...resizeStart.value.sel }
  const h = resizeHandle.value!

  let { x, y, w, h: ht } = s
  if (h.includes('e')) w = Math.max(10, s.w + dx)
  if (h.includes('s')) ht = Math.max(10, s.h + dy)
  if (h.includes('w')) { x = s.x + dx; w = Math.max(10, s.w - dx) }
  if (h.includes('n')) { y = s.y + dy; ht = Math.max(10, s.h - dy) }

  // 限制在屏幕范围内
  x = Math.max(0, x)
  y = Math.max(0, y)
  w = Math.min(w, screenW.value - x)
  ht = Math.min(ht, screenH.value - y)

  sel.value = { x, y, w, h: ht }
}

function isInsideSel(cx: number, cy: number) {
  const { x, y, w, h } = sel.value
  return cx >= x && cx <= x + w && cy >= y && cy <= y + h
}

// ── 标注绘制 ──────────────────────────────────────────────
function redraw(preview?: Shape | null) {
  const canvas = annotateCanvas.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')!
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.save()
  ctx.scale(dpr.value, dpr.value)

  for (const shape of shapes.value) drawShape(ctx, shape)
  if (preview) drawShape(ctx, preview)

  ctx.restore()
}

function drawShape(ctx: CanvasRenderingContext2D, shape: Shape) {
  const { type, x1, y1, x2, y2, color, lineWidth, text } = shape
  ctx.strokeStyle = color
  ctx.fillStyle = color
  ctx.lineWidth = lineWidth
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'

  if (type === 'rect') {
    ctx.strokeRect(x1, y1, x2 - x1, y2 - y1)
  } else if (type === 'line') {
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke()
  } else if (type === 'arrow') {
    drawArrow(ctx, x1, y1, x2, y2, lineWidth)
  } else if (type === 'text' && text) {
    ctx.font = `${Math.max(14, lineWidth * 6)}px -apple-system, sans-serif`
    ctx.fillText(text, x1, y1)
  } else if (type === 'blur') {
    const bw = Math.abs(x2 - x1), bh = Math.abs(y2 - y1)
    const bx = Math.min(x1, x2), by = Math.min(y1, y2)
    if (bw < 2 || bh < 2) return
    if (!bgImage.value) return
    ctx.save()
    // 裁剪到模糊区域，防止 blur 溢出到外部
    ctx.beginPath()
    ctx.rect(bx, by, bw, bh)
    ctx.clip()
    // 直接在裁剪区域内用 filter 绘制背景图对应区域
    // 注意：此时 ctx 已经 scale(dpr, dpr)，坐标是逻辑像素
    // bgImage 是物理像素图，所以 source 坐标要乘 dpr
    const blurPx = Math.max(8, lineWidth * 3)
    ctx.filter = `blur(${blurPx}px)`
    const sx = (sel.value.x + bx) * dpr.value
    const sy = (sel.value.y + by) * dpr.value
    // 多绘制一圈 blurPx 的边距，让 blur 边缘不透明
    const pad = blurPx
    ctx.drawImage(
      bgImage.value,
      sx - pad * dpr.value, sy - pad * dpr.value,
      (bw + pad * 2) * dpr.value, (bh + pad * 2) * dpr.value,
      bx - pad, by - pad,
      bw + pad * 2, bh + pad * 2,
    )
    ctx.restore()
    // 边框（不受 filter 影响）
    ctx.strokeRect(bx, by, bw, bh)
  }
}

function drawArrow(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number, lineWidth: number) {
  const angle = Math.atan2(y2 - y1, x2 - x1)
  // 箭头尺寸随线宽缩放
  const headLen = Math.max(12, lineWidth * 4)
  const headAngle = 0.42 // 约 24°

  // 箭头尖端坐标
  const tipX = x2
  const tipY = y2

  // 两侧翼点
  const wing1X = tipX - headLen * Math.cos(angle - headAngle)
  const wing1Y = tipY - headLen * Math.sin(angle - headAngle)
  const wing2X = tipX - headLen * Math.cos(angle + headAngle)
  const wing2Y = tipY - headLen * Math.sin(angle + headAngle)

  // 箭头底边中点（线段终点止于此，不穿过箭头）
  const baseX = (wing1X + wing2X) / 2
  const baseY = (wing1Y + wing2Y) / 2

  // 线段：从起点到箭头底边中点
  ctx.beginPath()
  ctx.moveTo(x1, y1)
  ctx.lineTo(baseX, baseY)
  ctx.stroke()

  // 实心箭头
  ctx.beginPath()
  ctx.moveTo(tipX, tipY)
  ctx.lineTo(wing1X, wing1Y)
  ctx.lineTo(wing2X, wing2Y)
  ctx.closePath()
  ctx.fill()
}

// ── 操作 ──────────────────────────────────────────────────
function setTool(tool: Tool) {
  activeTool.value = activeTool.value === tool ? null : tool
}

async function getAnnotationPng(): Promise<string> {
  if (!annotateCanvas.value || shapes.value.length === 0) return ''
  return annotateCanvas.value.toDataURL('image/png')
}

async function doCopy() {
  const ann = await getAnnotationPng()
  await invoke('copy_screenshot_to_clipboard', {
    selX: sel.value.x, selY: sel.value.y,
    selW: sel.value.w, selH: sel.value.h,
    scale: dpr.value,
    annotationPng: ann,
  })
  doCancel()
}

async function doSave() {
  const ann = await getAnnotationPng()
  const settings = useSettingsStore()
  // 使用配置的保存路径，空则使用下载文件夹
  const savePath = settings.screenshotSavePath || '~/Downloads'
  // 展开 ~ 为实际路径（Rust 端会处理目录自动生成文件名）
  const path = savePath.startsWith('~/')
    ? savePath.replace('~', await invoke<string>('get_home_dir').catch(() => ''))
    : savePath
  await invoke('save_screenshot', {
    selX: sel.value.x, selY: sel.value.y,
    selW: sel.value.w, selH: sel.value.h,
    scale: dpr.value,
    annotationPng: ann,
    path,
  })
  doCancel()
}

async function doOcr() {
  ocrText.value = ''
  ocrLoading.value = true
  const ann = await getAnnotationPng()
  try {
    const text = await invoke<string>('ocr_image', {
      selX: sel.value.x, selY: sel.value.y,
      selW: sel.value.w, selH: sel.value.h,
      scale: dpr.value,
      annotationPng: ann,
    })
    ocrText.value = text
  } catch (e) {
    ocrText.value = String(e)
  } finally {
    ocrLoading.value = false
  }
}

async function copyOcr() {
  if (ocrText.value) await writeText(ocrText.value)
}

function doCancel() {
  // 退出前将根容器 cursor 重置为 default，确保 WKWebView 在 overlay 卸载后立即刷新系统光标
  if (rootEl.value) rootEl.value.style.cursor = 'default'
  document.body.style.cursor = 'default'
  // 用 rAF 让浏览器先应用 cursor 变更，再卸载组件
  requestAnimationFrame(() => {
    document.body.style.cursor = ''
    emit('close')
  })
}

// ── 生命周期 ──────────────────────────────────────────────
function refocus() {
  rootEl.value?.focus()
}

// 由 Rust 全局鼠标监视器调用，更新十字线位置（绕过 WKWebView 在 Mission Control
// 返回后 mousemove 暂停的问题）
function setCrossPosition(cx: number, cy: number) {
  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${cx}px`)
    rootEl.value.style.setProperty('--cross-y', `${cy}px`)
  }
  crossX.value = cx
  crossY.value = cy
  if (phase.value === 'select' && !isDragging.value && !pendingDrag.value) {
    updateMagnifier(cx, cy)
  }
}
;(window as unknown as { __setScreenshotCross?: (x: number, y: number) => void })
  .__setScreenshotCross = setCrossPosition

onMounted(() => {
  // 用截图时的鼠标位置初始化十字线，确保挂载即对齐
  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${props.initialScreenshot.mouse_x}px`)
    rootEl.value.style.setProperty('--cross-y', `${props.initialScreenshot.mouse_y}px`)
  }
  // 用初始鼠标位置做一次窗口识别，避免必须移动鼠标才能高亮窗口
  hoverWindow.value = findWindowAt(props.initialScreenshot.mouse_x, props.initialScreenshot.mouse_y)
  nextTick(() => {
    updateMagnifier(props.initialScreenshot.mouse_x, props.initialScreenshot.mouse_y)
    refocus()
  })
  // Mission Control 等系统切换退出后窗口重新获得焦点时，
  // 自动 refocus rootEl，确保键盘事件正常响应
  window.addEventListener('focus', refocus)
})

onUnmounted(() => {
  window.removeEventListener('focus', refocus)
  delete (window as unknown as { __setScreenshotCross?: unknown }).__setScreenshotCross
})

watch(annotateCanvas, () => { if (annotateCanvas.value) redraw() })

</script>
