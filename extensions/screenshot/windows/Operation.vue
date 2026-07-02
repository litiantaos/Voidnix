<template>
  <div
    ref="rootEl"
    select="none"
    inset="0"
    fixed
    z="9999"
    overflow="hidden"
    :style="{ cursor: cursorStyle, pointerEvents: phase === 'scroll' ? 'none' : 'auto' }"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp($event)"
    @dblclick="onDoubleClick"
    @keydown="onKeyDown"
    tabindex="0"
  >
    <!-- 遮罩层 -->
    <template v-if="hasSelection">
      <div bg="black/45" pointer-events="none" fixed :style="maskTop" />
      <div bg="black/45" pointer-events="none" fixed :style="maskBottom" />
      <div bg="black/45" pointer-events="none" fixed :style="maskLeft" />
      <div bg="black/45" pointer-events="none" fixed :style="maskRight" />
    </template>
    <template v-else-if="phase === 'select' && hoverWindow">
      <div bg="black/45" pointer-events="none" fixed :style="hoverMaskTop" />
      <div bg="black/45" pointer-events="none" fixed :style="hoverMaskBottom" />
      <div bg="black/45" pointer-events="none" fixed :style="hoverMaskLeft" />
      <div bg="black/45" pointer-events="none" fixed :style="hoverMaskRight" />
    </template>
    <div v-else bg="black/45" pointer-events="none" inset="0" fixed />

    <!-- 十字线：1px 线 transform 居中在坐标点，消除线右/下偏 0.5px。
         select 阶段双轴；resize 边中控制点（n/s/e/w）仅显与边平齐的单轴，角控制点双轴 -->
    <div
      v-if="showCrossH"
      bg="accent/80"
      class="overlay-abs"
      style="left: 0; right: 0; height: 1px; top: var(--cross-y); transform: translateY(-0.5px)"
    />
    <div
      v-if="showCrossV"
      bg="accent/80"
      class="overlay-abs"
      style="top: 0; bottom: 0; width: 1px; left: var(--cross-x); transform: translateX(-0.5px)"
    />

    <!-- 放大镜 -->
    <div
      v-if="showMagnifier"
      border="~ black/20"
      rounded="lg"
      class="overlay-abs"
      shadow="xl"
      z="60"
      overflow="hidden"
      :style="magnifierStyle"
    >
      <div relative :style="{ width: MAGNIFIER_SIZE + 'px', height: MAGNIFIER_SIZE + 'px' }">
        <canvas
          :ref="setMagnifierCanvas"
          :width="MAGNIFIER_SIZE * dpr"
          :height="MAGNIFIER_SIZE * dpr"
          :style="{
            width: MAGNIFIER_SIZE + 'px',
            height: MAGNIFIER_SIZE + 'px',
          }"
          block
        />
        <div class="overlay-abs" inset="0">
          <div
            bg="accent/80"
            absolute
            :style="{
              left: 0,
              right: 0,
              height: '1px',
              top: `${MAGNIFIER_SIZE / 2}px`,
              transform: 'translateY(-0.5px)',
            }"
          />
          <div
            bg="accent/80"
            absolute
            :style="{
              top: 0,
              bottom: 0,
              width: '1px',
              left: `${MAGNIFIER_SIZE / 2}px`,
              transform: 'translateX(-0.5px)',
            }"
          />
        </div>
      </div>
      <div
        text="xs black"
        font="mono"
        border="t black/20"
        bg="white"
        flex
        gap="2"
        h="6"
        select="none"
        class="flex-center"
      >
        <div border="~ black/20" h="3" w="3" :style="{ background: pickedColor }"></div>
        <div>{{ pickedColor }}</div>
      </div>
    </div>

    <!-- 窗口高亮 -->
    <template v-if="!hasSelection && phase === 'select' && hoverWindow">
      <div class="overlay-abs" :style="[hoverWindowStyle, edgeOutline]">
        <div
          text="xs tx-primary"
          p="x-1.5 y-0.5"
          rounded
          bg="surface"
          class="overlay-abs"
          select="none"
          shadow
          :style="hoverSizeStyle"
        >
          {{ Math.round(hoverWindow.w) }}×{{ Math.round(hoverWindow.h) }}
        </div>
      </div>
    </template>

    <!-- 选区边框 + 8个控制点 -->
    <template v-if="hasSelection && phase !== 'scroll'">
      <div class="overlay-abs" :style="[selectionStyle, edgeOutline]">
        <div
          text="xs tx-primary"
          p="x-1.5 y-0.5"
          rounded
          bg="surface"
          class="overlay-abs"
          select="none"
          shadow
          :style="selSizeStyle"
        >
          {{ Math.round(sel.w) }}×{{ Math.round(sel.h) }}
        </div>
        <div
          v-for="h in handles"
          :key="h.id"
          border="~ accent"
          rounded="sm"
          bg="white"
          h="2"
          w="2"
          pointer-events="auto"
          absolute
          :style="h.style"
          @mouseenter="onHandleEnter(h.id)"
          @mouseleave="hoveredHandle = null"
          @mousedown.stop="onSelResizeStart(h.id, $event)"
        />
      </div>
    </template>

    <!-- 滚动截屏阶段：仅显示选区边框 -->
    <template v-if="hasSelection && phase === 'scroll'">
      <div class="overlay-abs" :style="[selectionStyle, edgeOutline]" />
    </template>

    <!-- 标注 canvas -->
    <canvas
      v-if="hasSelection && phase === 'annotate'"
      ref="annotateCanvas"
      class="overlay-abs"
      :style="{
        left: `${sel.x}px`,
        top: `${sel.y}px`,
        width: `${sel.w}px`,
        height: `${sel.h}px`,
      }"
      :width="sel.w * dpr"
      :height="sel.h * dpr"
    />
    <!-- 模糊选中边框（与控制点一体，DOM 层同步显示/隐藏） -->
    <div
      v-if="blurFrameStyle && phase === 'annotate'"
      border="~ accent dashed"
      class="overlay-abs"
      z="10"
      :style="blurFrameStyle"
    />

    <!-- 形状控制点覆盖层（选中形状 / 绘制中的 blur 形状时显示） -->
    <template v-if="effectiveShape && phase === 'annotate'">
      <!-- 矩形/模糊：8个控制点 + (矩形)圆角/旋转控制点 -->
      <template v-if="effectiveShape.type === 'rect' || effectiveShape.type === 'blur'">
        <div
          v-for="hp in shapeHandles"
          :key="hp.id"
          pointer-events="auto"
          absolute
          z="100"
          :class="{
            'cursor-ns-resize': hp.id === 'cr',
            'cursor-grab hover:cursor-grab active:cursor-grabbing': hp.id === 'rot',
          }"
          :style="hp.style"
          @mousedown.stop="startShapeHandleDrag(hp.id, $event)"
        >
          <!-- 圆角控制点：四段弧 + 透明命中区 -->
          <template v-if="hp.id === 'cr'">
            <div h="4" w="4" absolute class="-translate-x-1/2 -translate-y-1/2" />
            <svg
              class="overlay-abs -translate-x-1/2 -translate-y-1/2"
              width="11"
              height="11"
              viewBox="0 0 11 11"
            >
              <g stroke="#3b82f6" stroke-width="1.5" fill="none" stroke-linecap="round">
                <path d="M 3.4 1.45 A 4.25 4.25 0 0 1 7.6 1.45" />
                <path d="M 9.55 3.4 A 4.25 4.25 0 0 1 9.55 7.6" />
                <path d="M 7.6 9.55 A 4.25 4.25 0 0 1 3.4 9.55" />
                <path d="M 1.45 7.6 A 4.25 4.25 0 0 1 1.45 3.4" />
              </g>
            </svg>
          </template>
          <!-- 旋转控制点：环形箭头 -->
          <template v-else-if="hp.id === 'rot'">
            <div h="4" w="4" absolute class="-translate-x-1/2 -translate-y-1/2" />
            <svg
              class="overlay-abs -translate-x-1/2 -translate-y-1/2"
              width="12"
              height="12"
              viewBox="0 0 12 12"
            >
              <g
                stroke="#3b82f6"
                stroke-width="1.4"
                fill="none"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <!-- 3/4 圆弧（顶部断开） -->
                <path d="M 9.5 4 A 4 4 0 1 0 8.7 8.7" />
                <!-- 起始端箭头（指向左上） -->
                <path d="M 9.5 1.5 L 9.5 4 L 7 4" />
              </g>
            </svg>
          </template>
          <!-- 8个尺寸控制点：圆形 + 方向光标 -->
          <div
            v-else
            border="~ accent"
            rounded="full"
            bg="white"
            h="2.5"
            w="2.5"
            absolute
            class="-translate-x-1/2 -translate-y-1/2"
            :style="{
              cursor: handleCursor(hp.id, effectiveShape?.rotation ?? 0),
            }"
          />
        </div>
      </template>
      <!-- 直线/箭头：首尾2个控制点 -->
      <template v-else-if="effectiveShape.type === 'line' || effectiveShape.type === 'arrow'">
        <div
          v-for="hp in shapeHandles"
          :key="hp.id"
          pointer-events="auto"
          absolute
          z="100"
          :style="hp.style"
          @mousedown.stop="startShapeHandleDrag(hp.id, $event)"
        >
          <div
            border="~ accent"
            rounded="full"
            bg="white"
            h="2.5"
            w="2.5"
            cursor="move"
            absolute
            class="-translate-x-1/2 -translate-y-1/2"
          />
        </div>
      </template>
      <!-- 文本：只有右边中间一个控制点（调整宽度） -->
      <template v-else-if="effectiveShape.type === 'text'">
        <div
          v-for="hp in shapeHandles"
          :key="hp.id"
          pointer-events="auto"
          absolute
          z="100"
          :style="hp.style"
          @mousedown.stop="startShapeHandleDrag(hp.id, $event)"
        >
          <div
            border="~ accent"
            rounded="full"
            bg="white"
            h="2.5"
            w="2.5"
            cursor="ew-resize"
            absolute
            class="-translate-x-1/2 -translate-y-1/2"
          />
        </div>
      </template>
    </template>

    <!-- 文字输入框：外框可拖动移动，textarea 完全无边距与 canvas 对齐 -->
    <div
      v-if="textInput.visible"
      absolute
      z="50"
      :style="textInputWrapperStyle"
      @mousedown="onTextInputWrapperMouseDown"
    >
      <!-- 拖动边框：点击可拖动文本框 -->
      <div
        rounded="sm"
        cursor="move"
        inset="0"
        absolute
        :style="{ padding: '1px', border: '1px dashed #3b82f6' }"
        @mousedown.stop="startTextInputDrag($event)"
      />
      <textarea
        ref="textInputEl"
        v-model="textInput.value"
        outline="none"
        border="~ none"
        bg="transparent"
        w="full"
        block
        resize="none"
        relative
        z="1"
        :style="textInputInnerStyle"
        rows="1"
        @input="onTextInputInput"
        @keydown="onTextInputKeydown"
      />
    </div>

    <!-- 标注调色板 -->
    <AnnotationPalette
      ref="paletteRef"
      v-if="hasSelection && (phase === 'annotate' || phase === 'scroll') && !selResizeHandle"
      :sel="sel"
      :active-tool="activeTool"
      :color="annotColor"
      :line-width="annotLineWidth"
      :font-size="annotFontSize"
      :blur-amount="annotBlurAmount"
      :blur-mode="annotBlurMode"
      :screen-height="screenH"
      :screen-width="screenW"
      :mode="phase === 'scroll' ? 'scroll' : 'annotate'"
      class="pointer-events-auto"
      @tool="setTool"
      @color="annotColor = $event"
      @line-width="annotLineWidth = $event"
      @font-size="annotFontSize = $event"
      @blur-amount="annotBlurAmount = $event"
      @blur-mode="annotBlurMode = $event"
      @ocr="doOcr"
      @pin="doPin"
      @copy="doCopy"
      @save="doSave"
      @cancel="doCancel"
      @scroll-start="onScrollStart"
      @scroll-finish="onScrollFinish"
      @scroll-save="onScrollSave"
      @scroll-cancel="onScrollCancel"
    />

    <!-- 滚动截屏：右侧实时预览面板 -->
    <ScrollPreview
      v-if="phase === 'scroll'"
      class="pointer-events-auto"
      :sel="sel"
      :dpr="dpr"
      :preview-data-url="scrollCapture.previewDataUrl.value"
      :preview-width="scrollCapture.previewWidth.value"
      :preview-height="scrollCapture.previewHeight.value"
      :screen-width="screenW"
      :screen-height="screenH"
    />
    <!-- 自动停止提示 -->
    <div
      v-if="phase === 'scroll' && scrollCapture.atBottom.value"
      class="pointer-events-none left-1/2 fixed -translate-x-1/2"
      style="bottom: 20%"
    >
      <span class="text-xs ui-ctrl text-tx-secondary px-3 py-1.5 rounded-md">
        已到底部，按 Enter 完成
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import AnnotationPalette from './AnnotationPalette.vue'
import ScrollPreview from './ScrollPreview.vue'
import type { ScreenshotData, Phase } from '../composables/useTypes'
import { MAGNIFIER_SIZE, handleAbsolutePos } from '../composables/useTypes'
import { useSelection } from '../composables/useSelection'
import { useAnnotation } from '../composables/useAnnotation'
import { useTextInput } from '../composables/useTextInput'
import { useMagnifier } from '../composables/useMagnifier'
import { useShapeHandles } from '../composables/useShapeHandles'
import { useDrawing } from '../composables/useDrawing'
import { useMaskStyles } from '../composables/useMaskStyles'
import { useScreenshotActions } from '../composables/useScreenshotActions'
import { useOverlayEvents } from '../composables/useOverlayEvents'
import { useTextDetection } from '../composables/useTextDetection'
import { useScrollCapture } from '../composables/useScrollCapture'
import { wrapText } from '../composables/wrapText'

const props = defineProps<{ initialScreenshot: ScreenshotData }>()
const emit = defineEmits<{ close: [noRestoreFocus?: boolean] }>()

const rootEl = ref<HTMLElement>()
const annotateCanvas = ref<HTMLCanvasElement>()
const textInputEl = ref<HTMLTextAreaElement>()
const paletteRef = ref<InstanceType<typeof AnnotationPalette>>()

const screenW = ref(props.initialScreenshot.width)
const screenH = ref(props.initialScreenshot.height)
const dpr = ref(props.initialScreenshot.scale)
const windows = ref(props.initialScreenshot.windows ?? [])
const phase = ref<Phase>('select')
const hoveredHandle = ref<string | null>(null)
// 十字线分轴显示：select 阶段双轴；resize 时仅显与拖动边平齐的轴——
// n/s（水平边）显水平线，e/w（垂直边）显垂直线，角控制点双轴
const showCrossH = computed(() => {
  if (phase.value === 'select') return true
  const h = selection.selResizeHandle.value
  return !!h && (h.includes('n') || h.includes('s'))
})
const showCrossV = computed(() => {
  if (phase.value === 'select') return true
  const h = selection.selResizeHandle.value
  return !!h && (h.includes('e') || h.includes('w'))
})
// 放大窗额外覆盖「hover 控制点」：悬浮时只出放大窗，不出十字线
const showMagnifier = computed(
  () => phase.value === 'select' || !!selection.selResizeHandle.value || !!hoveredHandle.value,
)

// ── 组合 composables ──────────────────────────────────────
const selection = useSelection({ screenW, screenH, windows })
const annotation = useAnnotation()
const magnifier = useMagnifier({
  initialScreenshot: props.initialScreenshot,
  screenW,
  screenH,
  dpr,
  sel: selection.sel,
  hoveredHandle,
})
const textDetection = useTextDetection({ dpr })

// 绘制中的形状优先：控制点和选中框从开始拖动起就显示，松手后无缝切到 selectedShape。
// 仅当鼠标已移动（形状有实际尺寸）时才认作活动形状，避免按下鼠标的瞬间在点位置闪出零尺寸控制点。
// 文本是单击创建，没有拖动过程，故不在此处处理。
const effectiveShape = computed(() => {
  const s = annotation.currentShape.value
  if (annotation.isDrawing.value && s && s.type !== 'text') {
    if (s.x1 !== s.x2 || s.y1 !== s.y2) return s
  }
  return annotation.selectedShape.value
})

const drawing = useDrawing({
  annotateCanvas,
  dpr,
  sel: selection.sel,
  bgImage: magnifier.bgImage,
  shapes: annotation.shapes,
  textInput: ref({ visible: false, editingIndex: null }),
  textRegions: textDetection.textRegions,
})

const textInputComposable = useTextInput({
  sel: selection.sel,
  annotColor: annotation.annotColor,
  annotLineWidth: annotation.annotLineWidth,
  annotFontSize: annotation.annotFontSize,
  shapes: annotation.shapes,
  selectedShapeIndex: annotation.selectedShapeIndex,
  isHoveringSelectedShape: annotation.isHoveringSelectedShape,
  redraw: drawing.redraw,
  textInputEl,
})

const actions = useScreenshotActions({
  sel: selection.sel,
  dpr,
  shapes: annotation.shapes,
  annotateCanvas,
  bgImage: magnifier.bgImage,
  rootEl,
  emit,
})

const shapeHandlesComposable = useShapeHandles({
  sel: selection.sel,
  selectedShape: effectiveShape,
  selectedShapeIndex: annotation.selectedShapeIndex,
  textInput: textInputComposable.textInput,
  fromLocal: annotation.fromLocal,
  redraw: drawing.redraw,
})

const maskStyles = useMaskStyles({
  sel: selection.sel,
  hoverWindow: selection.hoverWindow,
  screenW,
  screenH,
  phase,
  selResizeHandle: selection.selResizeHandle,
  draggingShapeHandle: shapeHandlesComposable.draggingShapeHandle,
  selectedShape: annotation.selectedShape,
  isDraggingShape: annotation.isDraggingShape,
  isDraggingTextInput: textInputComposable.isDraggingTextInput,
  isHoveringSelectedShape: annotation.isHoveringSelectedShape,
  isHoveringAnyShape: annotation.isHoveringAnyShape,
  isDragging: selection.isDragging,
  activeTool: annotation.activeTool,
  crossX: magnifier.crossX,
  crossY: magnifier.crossY,
})

const events = useOverlayEvents({
  sel: selection.sel,
  dragStart: selection.dragStart,
  isDragging: selection.isDragging,
  pendingDrag: selection.pendingDrag,
  hoverWindow: selection.hoverWindow,
  selResizeHandle: selection.selResizeHandle,
  hasSelection: selection.hasSelection,
  findWindowAt: selection.findWindowAt,
  applySelResize: selection.applySelResize,
  isInsideSel: selection.isInsideSel,
  phase,
  activeTool: annotation.activeTool,
  annotColor: annotation.annotColor,
  annotLineWidth: annotation.annotLineWidth,
  annotBlurAmount: annotation.annotBlurAmount,
  annotBlurMode: annotation.annotBlurMode,
  shapes: annotation.shapes,
  currentShape: annotation.currentShape,
  isDrawing: annotation.isDrawing,
  drawStart: annotation.drawStart,
  selectedShapeIndex: annotation.selectedShapeIndex,
  isHoveringSelectedShape: annotation.isHoveringSelectedShape,
  isHoveringAnyShape: annotation.isHoveringAnyShape,
  isDraggingShape: annotation.isDraggingShape,
  shapeDragStart: annotation.shapeDragStart,
  draggingShapeHandle: shapeHandlesComposable.draggingShapeHandle,
  applyShapeHandleDrag: shapeHandlesComposable.applyShapeHandleDrag,
  textInput: textInputComposable.textInput,
  isDraggingTextInput: textInputComposable.isDraggingTextInput,
  textInputDragStart: textInputComposable.textInputDragStart,
  textInputPendingDrag: textInputComposable.textInputPendingDrag,
  openTextInput: textInputComposable.openTextInput,
  commitText: textInputComposable.commitText,
  cancelText: textInputComposable.cancelText,
  crossX: magnifier.crossX,
  crossY: magnifier.crossY,
  updateMagnifier: magnifier.updateMagnifier,
  pickedColor: magnifier.pickedColor,
  screenW,
  screenH,
  dpr,
  redraw: drawing.redraw,
  doCopy: actions.doCopy,
  doSave: actions.doSave,
  doOcr: actions.doOcr,
  doPin: actions.doPin,
  doCancel: actions.doCancel,
  onScrollCancel: () => onScrollCancelRef(),
  onScrollFinish: () => onScrollFinishRef(),
  rootEl,
})

// 转发：events 在 onScrollCancel/Finish 之前构造，用 ref 解决前向引用。
let onScrollCancelRef: () => Promise<void> | void = () => {}
let onScrollFinishRef: () => Promise<void> | void = () => {}

// ── 从 composables 解构模板需要的属性和方法 ──────────────────
const { sel, hasSelection, hoverWindow, selResizeHandle, startSelResize, applySelResize } =
  selection
const {
  activeTool,
  annotColor,
  annotLineWidth,
  annotFontSize,
  annotBlurAmount,
  annotBlurMode,
  handleCursor,
  setTool,
} = annotation
const {
  textInput,
  textInputWrapperStyle,
  textInputInnerStyle,
  startTextInputDrag,
  onTextInputWrapperMouseDown,
  onTextInputInput,
  onTextInputKeydown,
} = textInputComposable
const { pickedColor, crossX, crossY, magnifierStyle } = magnifier
function setMagnifierCanvas(el: unknown) {
  magnifier.magnifierCanvas.value = (el as HTMLCanvasElement | null) ?? undefined
}
const { shapeHandles, startShapeHandleDrag } = shapeHandlesComposable
const {
  selectionStyle,
  edgeOutline,
  hoverWindowStyle,
  hoverMaskTop,
  hoverMaskBottom,
  hoverMaskLeft,
  hoverMaskRight,
  maskTop,
  maskBottom,
  maskLeft,
  maskRight,
  selSizeStyle,
  hoverSizeStyle,
  handles,
  cursorStyle,
} = maskStyles
const { onMouseDown, onMouseMove, onMouseUp, onDoubleClick, onKeyDown } = events
const { doCopy, doSave, doOcr, doPin, doCancel } = actions

// 按下选区控制点即把十字线与选区边框都对齐到鼠标位置。
// 否则十字线停在上次 mousemove 的位置（鼠标在 8×8 控制点内，偏离选区边缘 grab offset），
// 边框仍在原位，两者差 grab offset；要等首次 mousemove 才 snap，视觉上"按下偏移、拉动正常"。
function onSelResizeStart(handle: string, e: MouseEvent) {
  // 进入 resize：清空 hover 锚定，放大窗改为跟随光标（与 select 首次拉选区一致）。
  // 否则翻转时 hoveredHandle 残留旧 id，放大窗位置锚定翻转后已位移的控制点而非光标。
  hoveredHandle.value = null
  rootEl.value?.style.setProperty('--cross-x', `${e.clientX}px`)
  rootEl.value?.style.setProperty('--cross-y', `${e.clientY}px`)
  crossX.value = e.clientX
  crossY.value = e.clientY
  const oldX = sel.value.x
  const oldY = sel.value.y
  startSelResize(handle, e)
  applySelResize(e.clientX, e.clientY)
  // snap 改变了 sel.x/y（w/n 控制点），反向平移标注保持其屏幕绝对位置（与拖动中一致）
  const dx = sel.value.x - oldX
  const dy = sel.value.y - oldY
  if (dx !== 0 || dy !== 0) {
    for (const s of annotation.shapes.value) {
      s.x1 -= dx
      s.x2 -= dx
      s.y1 -= dy
      s.y2 -= dy
    }
    drawing.redraw()
  }
  magnifier.updateMagnifier(e.clientX, e.clientY)
}

// 悬浮到选区控制点即显示放大窗（不出十字线）。位置与内容都以控制点为锚固定，
// 不受指针在控制点内的微动影响。
function onHandleEnter(id: string) {
  hoveredHandle.value = id
  const { x, y } = handleAbsolutePos(id, sel.value)
  // 放大窗由 hoveredHandle 触发 v-if 渲染，需等 canvas 挂载后再取色绘制
  nextTick(() => magnifier.updateMagnifier(x, y))
}

// ── 滚动截屏 ─────────────────────────────────────────────
const scrollCapture = useScrollCapture({ dpr })

async function onScrollStart() {
  if (!hasSelection.value) return
  // 进入滚动截屏阶段：清空已有标注（互斥关系）；选中态/绘制态全部复位
  annotation.shapes.value = []
  annotation.selectedShapeIndex.value = null
  annotation.activeTool.value = null
  annotation.isDrawing.value = false
  annotation.currentShape.value = null
  phase.value = 'scroll'
  await scrollCapture.start(sel.value)
  // 启动失败：回退到 annotate 阶段
  if (scrollCapture.error.value) {
    phase.value = 'annotate'
    return
  }
  // session 已创建，主动上报工具栏矩形（mouse_monitor 据此排除穿透）
  await nextTick()
  paletteRef.value?.reportToolbarRect()
  // 进入滚动模式后，根 div 必须保持可接收键盘事件以响应 Esc
  // 但鼠标在选区内的 mousedown 由原生 setIgnoresMouseEvents 放行，不会触达此 div
  nextTick(() => rootEl.value?.focus())
}

async function onScrollFinish() {
  // 完成：finish_scroll_capture 拿到 dataURL → 默认复制到剪贴板
  try {
    const dataUrl = await scrollCapture.finish()
    if (dataUrl) {
      await actions.doScrollCopy(dataUrl)
    } else {
      doCancel()
    }
  } catch (err) {
    console.error('[scroll] finish failed:', err)
    doCancel()
  }
}
onScrollFinishRef = onScrollFinish

async function onScrollSave() {
  try {
    const dataUrl = await scrollCapture.finish()
    if (dataUrl) {
      await actions.doScrollSave(dataUrl)
    } else {
      doCancel()
    }
  } catch (err) {
    console.error('[scroll] save failed:', err)
    doCancel()
  }
}

async function onScrollCancel() {
  await scrollCapture.cancel()
  doCancel()
}
onScrollCancelRef = onScrollCancel
// ── 模糊元素选中边框样式（DOM 层，与控制点天然同步） ────────
const blurFrameStyle = computed(() => {
  const s = effectiveShape.value
  if (!s || s.type !== 'blur') return null
  const x = Math.min(s.x1, s.x2)
  const y = Math.min(s.y1, s.y2)
  const w = Math.abs(s.x2 - s.x1)
  const h = Math.abs(s.y2 - s.y1)
  return {
    left: `${sel.value.x + x}px`,
    top: `${sel.value.y + y}px`,
    width: `${w}px`,
    height: `${h}px`,
  }
})

// ── 生命周期 ──────────────────────────────────────────────
function refocus() {
  rootEl.value?.focus()
}

function setCrossPosition(cx: number, cy: number) {
  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${cx}px`)
    rootEl.value.style.setProperty('--cross-y', `${cy}px`)
  }
  crossX.value = cx
  crossY.value = cy
  if (phase.value === 'select') magnifier.updateMagnifier(cx, cy)
}
;(
  window as unknown as { __setScreenshotCross?: (x: number, y: number) => void }
).__setScreenshotCross = setCrossPosition

onMounted(() => {
  // @mousedown 等监听已挂在 DOM 上，通知 Rust 解锁窗口鼠标事件并启动淡入。
  // 早于这一刻的点击会因 ignoresMouseEvents=true 击穿到底下应用，不会被吞。
  invoke(CMD.screenshotOverlayReady).catch(() => {})

  if (rootEl.value) {
    rootEl.value.style.setProperty('--cross-x', `${props.initialScreenshot.mouse_x}px`)
    rootEl.value.style.setProperty('--cross-y', `${props.initialScreenshot.mouse_y}px`)
  }
  selection.hoverWindow.value = selection.findWindowAt(
    props.initialScreenshot.mouse_x,
    props.initialScreenshot.mouse_y,
  )
  nextTick(() => {
    magnifier.updateMagnifier(props.initialScreenshot.mouse_x, props.initialScreenshot.mouse_y)
    refocus()
  })
  magnifier.loadPickerImage()
  window.addEventListener('focus', refocus)
  // ESC 兜底：用户点过画布/遮罩后焦点可能漂移到 body，根 div 的 @keydown 不再收
  // 到 ESC。挂在 window 上确保任何阶段都能退出。其他键仍由根 div 处理。
  window.addEventListener('keydown', onWindowKeyDown)
})

function onWindowKeyDown(e: KeyboardEvent) {
  if (e.key !== 'Escape') return
  // 已被根 div 的 @keydown 处理过的不再重复（事件冒泡到 window 时根 div 已处理）；
  // 这里只在根 div 没接到时兜底——通过 activeElement 判断。
  if (document.activeElement === rootEl.value) return
  events.onKeyDown(e)
}

onUnmounted(() => {
  window.removeEventListener('focus', refocus)
  window.removeEventListener('keydown', onWindowKeyDown)
  delete (window as unknown as { __setScreenshotCross?: unknown }).__setScreenshotCross
})

watch(annotateCanvas, () => {
  if (annotateCanvas.value) drawing.redraw()
})

watch(annotation.annotBlurAmount, (v) => {
  const s = annotation.selectedShape.value
  if (s && s.type === 'blur') {
    s.blurAmount = v
    drawing.redraw()
  }
})

// 调字号：若当前选中是 text，同步更新字号并按新字号重新换行。
// 若正在编辑该形状，textarea 高度需要按新字号重算。
watch(annotation.annotFontSize, (v) => {
  const s = annotation.selectedShape.value
  if (s && s.type === 'text') {
    s.fontSize = v
    if (s.text) {
      const font = `${v}px -apple-system, sans-serif`
      s.textLines = wrapText(s.text, s.textWidth ?? 160, font)
    }
    drawing.redraw()
    const ti = textInputComposable.textInput.value
    if (ti.visible && ti.editingIndex === annotation.selectedShapeIndex.value) {
      nextTick(() => textInputComposable.autoResizeTextInput())
    }
  }
})

// 调线宽：若当前选中是 rect/line/arrow，同步更新。
watch(annotation.annotLineWidth, (v) => {
  const s = annotation.selectedShape.value
  if (s && (s.type === 'rect' || s.type === 'line' || s.type === 'arrow')) {
    s.lineWidth = v
    drawing.redraw()
  }
})

watch(annotation.selectedShapeIndex, (idx) => {
  if (idx === null) return
  const s = annotation.shapes.value[idx]
  if (!s) return
  // 选中 blur：同步 blur 参数
  if (s.type === 'blur') {
    if (typeof s.blurAmount === 'number') annotation.annotBlurAmount.value = s.blurAmount
    const mode = s.blurMode ?? 'selection'
    annotation.annotBlurMode.value = mode
    if (mode === 'text') textDetection.detect()
  }
  // 选中 text：同步字号
  if (s.type === 'text' && typeof s.fontSize === 'number') {
    annotation.annotFontSize.value = s.fontSize
  }
  // 选中 rect/line/arrow：同步线宽
  if (s.type === 'rect' || s.type === 'line' || s.type === 'arrow') {
    annotation.annotLineWidth.value = s.lineWidth
  }
})

// 模糊模式切换：更新当前选中 shape；切到文本模式时按需触发检测。
watch(annotation.annotBlurMode, (mode) => {
  const s = annotation.selectedShape.value
  if (s && s.type === 'blur') {
    s.blurMode = mode
    drawing.redraw()
  }
  if (mode === 'text') textDetection.detect()
})

// 选中 blur 工具且为文本模式时，预热检测（用户开始画框前完成）。
watch(annotation.activeTool, (tool) => {
  if (tool === 'blur' && annotation.annotBlurMode.value === 'text') {
    textDetection.detect()
  }
})

// 进入标注阶段即预热文本检测，把 Swift 冷启动放在「框选完成 → 选择工具 → 拖动」
// 这段空闲里，避免用户第一次拉文本模糊选区时看到延迟。
watch(
  phase,
  (p) => {
    if (p === 'annotate') textDetection.detect()
  },
  { immediate: true },
)

// 文本区域更新后，重绘画布以应用最新结果。
watch(textDetection.textRegions, () => drawing.redraw(), { deep: true })
</script>
