<template>
  <div
    ref="rootEl"
    class="select-none inset-0 fixed z-9999 overflow-hidden"
    :style="{ cursor: cursorStyle }"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp($event)"
    @dblclick="onDoubleClick"
    @keydown="onKeyDown"
    tabindex="0"
  >
    <!-- 遮罩层 -->
    <template v-if="hasSelection">
      <div class="bg-black/45 pointer-events-none fixed" :style="maskTop" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskBottom" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskLeft" />
      <div class="bg-black/45 pointer-events-none fixed" :style="maskRight" />
    </template>
    <template v-else-if="phase === 'select' && hoverWindow">
      <div
        class="bg-black/45 pointer-events-none fixed"
        :style="hoverMaskTop"
      />
      <div
        class="bg-black/45 pointer-events-none fixed"
        :style="hoverMaskBottom"
      />
      <div
        class="bg-black/45 pointer-events-none fixed"
        :style="hoverMaskLeft"
      />
      <div
        class="bg-black/45 pointer-events-none fixed"
        :style="hoverMaskRight"
      />
    </template>
    <div v-else class="bg-black/45 pointer-events-none inset-0 fixed" />

    <!-- 十字线 -->
    <template v-if="phase === 'select'">
      <div
        class="bg-accent/80 pointer-events-none absolute"
        style="left: 0; right: 0; height: 1px; top: var(--cross-y)"
      />
      <div
        class="bg-accent/80 pointer-events-none absolute"
        style="top: 0; bottom: 0; width: 1px; left: var(--cross-x)"
      />
    </template>

    <!-- 放大镜 -->
    <div
      v-if="phase === 'select'"
      class="border border-black/20 rounded-lg pointer-events-none shadow-xl absolute z-50 overflow-hidden"
      :style="magnifierStyle"
    >
      <div
        class="relative"
        :style="{ width: MAGNIFIER_SIZE + 'px', height: MAGNIFIER_SIZE + 'px' }"
      >
        <canvas
          :ref="setMagnifierCanvas"
          :width="MAGNIFIER_SIZE * dpr"
          :height="MAGNIFIER_SIZE * dpr"
          :style="{
            width: MAGNIFIER_SIZE + 'px',
            height: MAGNIFIER_SIZE + 'px',
          }"
          class="block"
        />
        <div class="pointer-events-none inset-0 absolute">
          <div
            class="bg-accent/80 absolute"
            :style="{
              left: 0,
              right: 0,
              height: '1px',
              top: `${MAGNIFIER_SIZE / 2}px`,
            }"
          />
          <div
            class="bg-accent/80 absolute"
            :style="{
              top: 0,
              bottom: 0,
              width: '1px',
              left: `${MAGNIFIER_SIZE / 2}px`,
            }"
          />
        </div>
      </div>
      <div
        class="text-xs text-black font-mono border-t border-black/20 bg-white flex gap-2 h-6 select-none items-center justify-center"
      >
        <div
          class="border border-black/20 h-3 w-3"
          :style="{ background: pickedColor }"
        ></div>
        <div>{{ pickedColor }}</div>
      </div>
    </div>

    <!-- 窗口高亮 -->
    <template v-if="!hasSelection && phase === 'select' && hoverWindow">
      <div
        class="border border-accent pointer-events-none absolute"
        :style="hoverWindowStyle"
      >
        <div
          class="text-xs text-tx-primary px-1.5 py-0.5 rounded bg-surface pointer-events-none select-none shadow absolute"
          :style="hoverSizeStyle"
        >
          {{ Math.round(hoverWindow.w) }}×{{ Math.round(hoverWindow.h) }}
        </div>
      </div>
    </template>

    <!-- 选区边框 + 8个控制点 -->
    <template v-if="hasSelection">
      <div
        class="border border-accent pointer-events-none absolute"
        :style="selectionStyle"
      >
        <div
          class="text-xs text-tx-primary px-1.5 py-0.5 rounded bg-surface pointer-events-none select-none shadow absolute"
          :style="selSizeStyle"
        >
          {{ Math.round(sel.w) }}×{{ Math.round(sel.h) }}
        </div>
        <div
          v-for="h in handles"
          :key="h.id"
          class="border border-accent rounded-sm bg-white h-2 w-2 pointer-events-auto absolute"
          :style="h.style"
          @mousedown.stop="startSelResize(h.id, $event)"
        />
      </div>
    </template>

    <!-- 标注 canvas -->
    <canvas
      v-if="hasSelection && phase === 'annotate'"
      ref="annotateCanvas"
      class="pointer-events-none absolute"
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
      v-if="selectedShapeIndex !== null && phase === 'annotate' && selectedShape?.type === 'blur'"
      class="border border-accent border-dashed pointer-events-none absolute z-10"
      :style="blurSelectionStyle"
    />

    <!-- 形状控制点覆盖层（选中形状时显示） -->
    <template v-if="selectedShapeIndex !== null && phase === 'annotate'">
      <!-- 矩形/模糊：8个控制点 + (矩形)圆角/旋转控制点 -->
      <template
        v-if="
          selectedShape &&
          (selectedShape.type === 'rect' || selectedShape.type === 'blur')
        "
      >
        <div
          v-for="hp in shapeHandles"
          :key="hp.id"
          class="pointer-events-auto absolute z-100"
          :class="{
            'cursor-ns-resize': hp.id === 'cr',
            'cursor-grab hover:cursor-grab active:cursor-grabbing':
              hp.id === 'rot',
          }"
          :style="hp.style"
          @mousedown.stop="startShapeHandleDrag(hp.id, $event)"
        >
          <!-- 圆角控制点：四段弧 + 透明命中区 -->
          <template v-if="hp.id === 'cr'">
            <div class="h-4 w-4 absolute -translate-x-1/2 -translate-y-1/2" />
            <svg
              class="pointer-events-none absolute -translate-x-1/2 -translate-y-1/2"
              width="11"
              height="11"
              viewBox="0 0 11 11"
            >
              <g
                stroke="#3b82f6"
                stroke-width="1.5"
                fill="none"
                stroke-linecap="round"
              >
                <path d="M 3.4 1.45 A 4.25 4.25 0 0 1 7.6 1.45" />
                <path d="M 9.55 3.4 A 4.25 4.25 0 0 1 9.55 7.6" />
                <path d="M 7.6 9.55 A 4.25 4.25 0 0 1 3.4 9.55" />
                <path d="M 1.45 7.6 A 4.25 4.25 0 0 1 1.45 3.4" />
              </g>
            </svg>
          </template>
          <!-- 旋转控制点：环形箭头 -->
          <template v-else-if="hp.id === 'rot'">
            <div class="h-4 w-4 absolute -translate-x-1/2 -translate-y-1/2" />
            <svg
              class="pointer-events-none absolute -translate-x-1/2 -translate-y-1/2"
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
            class="border border-accent rounded-full bg-white h-2.5 w-2.5 absolute -translate-x-1/2 -translate-y-1/2"
            :style="{
              cursor: handleCursor(hp.id, selectedShape?.rotation ?? 0),
            }"
          />
        </div>
      </template>
      <!-- 直线/箭头：首尾2个控制点 -->
      <template
        v-else-if="
          selectedShape &&
          (selectedShape.type === 'line' || selectedShape.type === 'arrow')
        "
      >
        <div
          v-for="hp in shapeHandles"
          :key="hp.id"
          class="pointer-events-auto absolute z-100"
          :style="hp.style"
          @mousedown.stop="startShapeHandleDrag(hp.id, $event)"
        >
          <div
            class="border border-accent rounded-full bg-white h-2.5 w-2.5 cursor-move absolute -translate-x-1/2 -translate-y-1/2"
          />
        </div>
      </template>
      <!-- 文本：只有右边中间一个控制点（调整宽度） -->
      <template v-else-if="selectedShape && selectedShape.type === 'text'">
        <div
          v-for="hp in shapeHandles"
          :key="hp.id"
          class="pointer-events-auto absolute z-100"
          :style="hp.style"
          @mousedown.stop="startShapeHandleDrag(hp.id, $event)"
        >
          <div
            class="border border-accent rounded-full bg-white h-2.5 w-2.5 cursor-ew-resize absolute -translate-x-1/2 -translate-y-1/2"
          />
        </div>
      </template>
    </template>

    <!-- 文字输入框：外框可拖动移动，textarea 完全无边距与 canvas 对齐 -->
    <div
      v-if="textInput.visible"
      class="absolute z-50"
      :style="textInputWrapperStyle"
      @mousedown="onTextInputWrapperMouseDown"
    >
      <!-- 拖动边框：点击可拖动文本框 -->
      <div
        class="rounded-sm cursor-move inset-0 absolute"
        :style="{ padding: '1px', border: '1px dashed rgba(0,0,0,0.35)' }"
        @mousedown.stop="startTextInputDrag($event)"
      />
      <textarea
        ref="textInputEl"
        v-model="textInput.value"
        class="outline-none border-none bg-transparent w-full block resize-none relative z-1"
        :style="textInputInnerStyle"
        rows="1"
        @input="onTextInputInput"
        @keydown="onTextInputKeydown"
      />
    </div>

    <!-- 标注调色板 -->
    <AnnotationPalette
      v-if="hasSelection && phase === 'annotate'"
      :sel="sel"
      :active-tool="activeTool"
      :color="annotColor"
      :line-width="annotLineWidth"
      :blur-amount="annotBlurAmount"
      :screen-height="screenH"
      :screen-width="screenW"
      @tool="setTool"
      @color="annotColor = $event"
      @line-width="annotLineWidth = $event"
      @blur-amount="annotBlurAmount = $event"
      @ocr="doOcr"
      @pin="doPin"
      @copy="doCopy"
      @save="doSave"
      @cancel="doCancel"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import AnnotationPalette from './AnnotationPalette.vue'
import type { ScreenshotData } from '../composables/useTypes'
import { MAGNIFIER_SIZE } from '../composables/useTypes'
import { useSelection } from '../composables/useSelection'
import { useAnnotation } from '../composables/useAnnotation'
import { useTextInput } from '../composables/useTextInput'
import { useMagnifier } from '../composables/useMagnifier'
import { useShapeHandles } from '../composables/useShapeHandles'
import { useDrawing } from '../composables/useDrawing'
import { useMaskStyles } from '../composables/useMaskStyles'
import { useScreenshotActions } from '../composables/useScreenshotActions'
import { useOverlayEvents } from '../composables/useOverlayEvents'

const props = defineProps<{ initialScreenshot: ScreenshotData }>()
const emit = defineEmits<{ (e: 'close', forOcr?: boolean): void }>()

const rootEl = ref<HTMLElement>()
const annotateCanvas = ref<HTMLCanvasElement>()
const textInputEl = ref<HTMLTextAreaElement>()

const screenW = ref(props.initialScreenshot.width)
const screenH = ref(props.initialScreenshot.height)
const dpr = ref(props.initialScreenshot.scale)
const windows = ref(props.initialScreenshot.windows ?? [])
const phase = ref<'select' | 'annotate'>('select')

// ── 组合 composables ──────────────────────────────────────
const selection = useSelection({ screenW, screenH, windows })
const annotation = useAnnotation()
const magnifier = useMagnifier({ initialScreenshot: props.initialScreenshot, screenW, screenH, dpr })
const drawing = useDrawing({
  annotateCanvas,
  dpr,
  sel: selection.sel,
  bgImage: magnifier.bgImage,
  shapes: annotation.shapes,
  textInput: ref({ visible: false, editingIndex: null }),
})

const textInputComposable = useTextInput({
  sel: selection.sel,
  annotColor: annotation.annotColor,
  annotLineWidth: annotation.annotLineWidth,
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
  selectedShape: annotation.selectedShape,
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
  selResizeHandle: selection.selResizeHandle,
  draggingShapeHandle: shapeHandlesComposable.draggingShapeHandle,
  selectedShape: annotation.selectedShape,
  isDraggingShape: annotation.isDraggingShape,
  isDraggingTextInput: textInputComposable.isDraggingTextInput,
  isHoveringSelectedShape: annotation.isHoveringSelectedShape,
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
  shapes: annotation.shapes,
  currentShape: annotation.currentShape,
  isDrawing: annotation.isDrawing,
  drawStart: annotation.drawStart,
  selectedShapeIndex: annotation.selectedShapeIndex,
  isHoveringSelectedShape: annotation.isHoveringSelectedShape,
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
  rootEl,
})

// ── 从 composables 解构模板需要的属性和方法 ──────────────────
const { sel, hasSelection, hoverWindow, startSelResize } = selection
const {
  activeTool,
  annotColor,
  annotLineWidth,
  annotBlurAmount,
  selectedShapeIndex,
  selectedShape,
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
const {
  pickedColor,
  crossX,
  crossY,
  magnifierStyle,
} = magnifier
function setMagnifierCanvas(el: unknown) {
  magnifier.magnifierCanvas.value = (el as HTMLCanvasElement | null) ?? undefined
}
const {
  shapeHandles,
  startShapeHandleDrag,
} = shapeHandlesComposable
const {
  selectionStyle,
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
const {
  onMouseDown,
  onMouseMove,
  onMouseUp,
  onDoubleClick,
  onKeyDown,
} = events
const { doCopy, doSave, doOcr, doPin, doCancel } = actions
// ── 模糊元素选中边框样式（DOM 层，与控制点天然同步） ────────
const blurSelectionStyle = computed(() => {
  if (!selectedShape.value || selectedShape.value.type !== 'blur') return {}
  const s = selectedShape.value
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
  if (phase.value === 'select')
    magnifier.updateMagnifier(cx, cy)
}
;(
  window as unknown as { __setScreenshotCross?: (x: number, y: number) => void }
).__setScreenshotCross = setCrossPosition

onMounted(() => {
  if (rootEl.value) {
    rootEl.value.style.setProperty(
      '--cross-x',
      `${props.initialScreenshot.mouse_x}px`,
    )
    rootEl.value.style.setProperty(
      '--cross-y',
      `${props.initialScreenshot.mouse_y}px`,
    )
  }
  selection.hoverWindow.value = selection.findWindowAt(
    props.initialScreenshot.mouse_x,
    props.initialScreenshot.mouse_y,
  )
  nextTick(() => {
    magnifier.updateMagnifier(
      props.initialScreenshot.mouse_x,
      props.initialScreenshot.mouse_y,
    )
    refocus()
  })
  magnifier.loadPickerImage()
  window.addEventListener('focus', refocus)
})

onUnmounted(() => {
  window.removeEventListener('focus', refocus)
  delete (window as unknown as { __setScreenshotCross?: unknown })
    .__setScreenshotCross
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

watch(annotation.selectedShapeIndex, (idx) => {
  if (idx === null) return
  const s = annotation.shapes.value[idx]
  if (s && s.type === 'blur' && typeof s.blurAmount === 'number') {
    annotation.annotBlurAmount.value = s.blurAmount
  }
})
</script>
