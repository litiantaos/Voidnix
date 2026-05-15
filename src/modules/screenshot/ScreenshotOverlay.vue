<template>
  <div
    ref="rootEl"
    class="inset-0 fixed z-[9999] overflow-hidden"
    :style="{ cursor: cursorStyle }"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @keydown="onKeyDown"
    tabindex="0"
  >
    <!-- 截图背景 -->
    <img
      :src="screenshot.data_url"
      class="h-full w-full inset-0 fixed"
      style="image-rendering: pixelated; pointer-events: none; user-select: none;"
      draggable="false"
    />

    <!-- 遮罩层（选区外置灰，选区内透明） -->
    <template v-if="hasSelection">
      <div class="bg-black/45 pointer-events-none fixed" :style="maskTop" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskBottom" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskLeft" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskRight" />
    </template>
    <div v-else class="bg-black/45 pointer-events-none inset-0 fixed" />

    <!-- 十字线（未确定选区时）：CSS custom property 设在根容器上，挂载即有正确初始值 -->
    <template v-if="!hasSelection && phase === 'select'">
      <div ref="crossH" class="bg-white/60 pointer-events-none absolute" style="left: 0; right: 0; height: 1px; top: var(--cross-y)" />
      <div ref="crossV" class="bg-white/60 pointer-events-none absolute" style="top: 0; bottom: 0; width: 1px; left: var(--cross-x)" />
    </template>

    <!-- 选区边框 + 控制点 -->
    <template v-if="hasSelection">
      <div
        class="border border-accent pointer-events-none absolute"
        :style="selectionStyle"
      >
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

    <!-- 工具栏 -->
    <ScreenshotToolbar
      v-if="hasSelection"
      :sel="sel"
      :active-tool="activeTool"
      :color="annotColor"
      :line-width="annotLineWidth"
      :screen-height="screenH"
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
          <button class="text-xs px-2 py-0.5 rounded ui-clickable" @click="copyOcr">复制</button>
          <button class="text-xs px-2 py-0.5 rounded ui-clickable" @click="ocrText = null">✕</button>
        </div>
      </div>
      <div v-if="ocrLoading" class="text-tx-muted">识别中…</div>
      <div v-else>{{ ocrText || '未识别到文字' }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { save as saveDialog } from '@tauri-apps/plugin-dialog'
import ScreenshotToolbar from './ScreenshotToolbar.vue'

interface ScreenshotData { data_url: string; width: number; height: number; scale: number }
interface Sel { x: number; y: number; w: number; h: number }

const props = defineProps<{ initialScreenshot: ScreenshotData }>()
const emit = defineEmits<{ (e: 'close'): void }>()

const rootEl = ref<HTMLElement>()
const annotateCanvas = ref<HTMLCanvasElement>()

const screenshot = ref<ScreenshotData>(props.initialScreenshot)
const screenW = ref(props.initialScreenshot.width)
const screenH = ref(props.initialScreenshot.height)
const dpr = ref(props.initialScreenshot.scale)

// 选区状态
type Phase = 'select' | 'annotate'
const phase = ref<Phase>('select')
const sel = ref<Sel>({ x: 0, y: 0, w: 0, h: 0 })
const dragStart = ref({ x: 0, y: 0 })
const isDragging = ref(false)
const resizeHandle = ref<string | null>(null)
const resizeStart = ref({ x: 0, y: 0, sel: { x: 0, y: 0, w: 0, h: 0 } })
const mouse = ref({ x: 0, y: 0 })

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
  if (phase.value === 'select') return 'none'
  if (activeTool.value === 'text') return 'text'
  if (activeTool.value) return 'crosshair'
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

// ── 鼠标事件 ──────────────────────────────────────────────
function onMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  const { clientX: cx, clientY: cy } = e

  if (phase.value === 'select') {
    isDragging.value = true
    dragStart.value = { x: cx, y: cy }
    sel.value = { x: cx, y: cy, w: 0, h: 0 }
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
    const text = prompt('输入文字')
    if (text) {
      shapes.value.push({
        type: 'text', x1: cx - sel.value.x, y1: cy - sel.value.y,
        x2: cx - sel.value.x, y2: cy - sel.value.y,
        color: annotColor.value, lineWidth: annotLineWidth.value, text,
      })
      redraw()
    }
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
  mouse.value = { x: cx, y: cy }
  // 直接在根容器上更新十字线位置，绕过 Vue 响应式批处理延迟
  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${cx}px`)
    rootEl.value.style.setProperty('--cross-y', `${cy}px`)
  }

  if (phase.value === 'select' && isDragging.value) {
    const x = Math.min(cx, dragStart.value.x)
    const y = Math.min(cy, dragStart.value.y)
    const w = Math.abs(cx - dragStart.value.x)
    const h = Math.abs(cy - dragStart.value.y)
    sel.value = { x, y, w, h }
    return
  }

  if (resizeHandle.value) {
    applyResize(cx, cy)
    return
  }

  if (phase.value === 'annotate' && isDragging.value && !activeTool.value) {
    sel.value.x = cx - dragStart.value.x
    sel.value.y = cy - dragStart.value.y
    return
  }

  if (isDrawing.value && currentShape.value) {
    currentShape.value.x2 = cx - sel.value.x
    currentShape.value.y2 = cy - sel.value.y
    redraw(currentShape.value)
  }
}

function onMouseUp(_e: MouseEvent) {
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
    drawArrow(ctx, x1, y1, x2, y2)
  } else if (type === 'text' && text) {
    ctx.font = `${Math.max(14, lineWidth * 6)}px -apple-system, sans-serif`
    ctx.fillText(text, x1, y1)
  } else if (type === 'blur') {
    const bw = Math.abs(x2 - x1), bh = Math.abs(y2 - y1)
    const bx = Math.min(x1, x2), by = Math.min(y1, y2)
    ctx.save()
    ctx.filter = 'blur(8px)'
    // 从截图中取对应区域绘制模糊
    if (screenshot.value) {
      const img = new Image()
      img.src = screenshot.value.data_url
      const sx = (sel.value.x + bx) * dpr.value
      const sy = (sel.value.y + by) * dpr.value
      ctx.drawImage(img, sx, sy, bw * dpr.value, bh * dpr.value, bx, by, bw, bh)
    }
    ctx.restore()
    ctx.strokeRect(bx, by, bw, bh)
  }
}

function drawArrow(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
  const angle = Math.atan2(y2 - y1, x2 - x1)
  const len = 12
  ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke()
  ctx.beginPath()
  ctx.moveTo(x2, y2)
  ctx.lineTo(x2 - len * Math.cos(angle - 0.4), y2 - len * Math.sin(angle - 0.4))
  ctx.lineTo(x2 - len * Math.cos(angle + 0.4), y2 - len * Math.sin(angle + 0.4))
  ctx.closePath(); ctx.fill()
}

// ── 操作 ──────────────────────────────────────────────────
function setTool(tool: Tool) {
  activeTool.value = activeTool.value === tool ? null : tool
}

async function getAnnotatedDataUrl(): Promise<string> {
  if (!screenshot.value) return ''
  const { x, y, w, h } = sel.value
  const sc = screenshot.value.scale

  // 合成：截图区域 + 标注
  const offscreen = document.createElement('canvas')
  offscreen.width = w * sc
  offscreen.height = h * sc
  const ctx = offscreen.getContext('2d')!

  const img = new Image()
  img.src = screenshot.value.data_url
  await new Promise(r => { img.onload = r })
  ctx.drawImage(img, x * sc, y * sc, w * sc, h * sc, 0, 0, w * sc, h * sc)

  // 叠加标注
  if (annotateCanvas.value) {
    ctx.drawImage(annotateCanvas.value, 0, 0, w * sc, h * sc)
  }

  return offscreen.toDataURL('image/png')
}

async function doCopy() {
  const dataUrl = await getAnnotatedDataUrl()
  await invoke('copy_screenshot_to_clipboard', { imageData: dataUrl })
  doCancel()
}

async function doSave() {
  const dataUrl = await getAnnotatedDataUrl()
  const path = await saveDialog({
    defaultPath: `screenshot_${Date.now()}.png`,
    filters: [{ name: 'PNG', extensions: ['png'] }],
  })
  if (path) {
    await invoke('save_screenshot', { imageData: dataUrl, path })
  }
  doCancel()
}

async function doOcr() {
  ocrText.value = ''
  ocrLoading.value = true
  const dataUrl = await getAnnotatedDataUrl()
  try {
    const text = await invoke<string>('ocr_image', { imageData: dataUrl })
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
  // 重置 body cursor，防止 WKWebView 在 overlay 卸载后不刷新系统光标
  document.body.style.cursor = ''
  emit('close')
}

// ── 生命周期 ──────────────────────────────────────────────
onMounted(() => {
  // 初始化十字线到屏幕中心，确保挂载时就有合理位置
  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${screenW.value / 2}px`)
    rootEl.value.style.setProperty('--cross-y', `${screenH.value / 2}px`)
  }
  nextTick(() => rootEl.value?.focus())
})

watch(annotateCanvas, () => { if (annotateCanvas.value) redraw() })

</script>
