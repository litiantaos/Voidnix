<template>
  <div
    ref="rootEl"
    select="none"
    inset="0"
    fixed
    z="9999"
    overflow="hidden"
    :style="{ cursor: cursorStyle, pointerEvents: phase === 'scroll' ? 'none' : 'auto' }"
    @mousedown="onDomMouseDown"
    @mousemove="onDomMouseMove"
    @mouseup="onDomMouseUp($event)"
    @dblclick="onDoubleClick"
    @keydown="onKeyDown"
    tabindex="0"
  >
    <SelectionChrome
      :phase="phase"
      :has-selection="hasSelection"
      :hover-window="hoverWindow"
      :sel="sel"
      :show-cross-h="showCrossH"
      :show-cross-v="showCrossV"
      :mask-top="maskTop"
      :mask-bottom="maskBottom"
      :mask-left="maskLeft"
      :mask-right="maskRight"
      :hover-mask-top="hoverMaskTop"
      :hover-mask-bottom="hoverMaskBottom"
      :hover-mask-left="hoverMaskLeft"
      :hover-mask-right="hoverMaskRight"
      :selection-style="selectionStyle"
      :edge-outline="edgeOutline"
      :hover-window-style="hoverWindowStyle"
      :sel-size-style="selSizeStyle"
      :hover-size-style="hoverSizeStyle"
      :handles="handles"
      @handle-enter="onHandleEnter"
      @handle-leave="hoveredHandle = null"
      @handle-resize="onSelResizeStart"
    />

    <!-- 放大镜 -->
    <div
      v-if="showMagnifier"
      class="border border-soft radius-panel border-solid overlay-abs"
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
        text="xs primary"
        font="mono"
        class="border-t border-soft flex-center"
        bg="white"
        flex
        gap="2"
        h="6"
        select="none"
      >
        <div
          class="border border-soft border-solid"
          h="3"
          w="3"
          :style="{ background: pickedColor }"
        ></div>
        <div>{{ pickedColor }}</div>
      </div>
    </div>

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

    <ShapeHandlesOverlay
      :shape="effectiveShape"
      :phase="phase"
      :handles="shapeHandles"
      :handle-cursor="handleCursor"
      @drag="startShapeHandleDrag"
    />

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
        :style="{ padding: '1px', border: '1px dashed var(--color-accent)' }"
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

    <!-- 标注调色板：进出场统一走 ui-popup -->
    <Transition name="ui-popup" appear>
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
    </Transition>

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
    <!-- 选区阶段：快捷键提示（无工具栏时） -->
    <Transition name="ui-popup" appear>
      <div
        v-if="phase === 'select'"
        class="pointer-events-none bottom-6 left-1/2 fixed z-60 -translate-x-1/2"
      >
        <div text="xs secondary" p="1.5" flex gap="1.5" items="center" class="mica-panel">
          <span v-for="tip in selectShortcutTips" :key="tip.key" flex gap="1.5" items="center">
            <kbd
              text="xs primary"
              font="medium mono"
              rounded
              class="fill-ctrl"
              flex
              h="5"
              min-w="5"
              px="1.5"
              items="center"
              justify="center"
            >
              {{ tip.key }}
            </kbd>
            <span>{{ tip.label }}</span>
          </span>
        </div>
      </div>
    </Transition>

    <!-- 自动停止提示 -->
    <div
      v-if="phase === 'scroll' && scrollCapture.atBottom.value"
      class="pointer-events-none left-1/2 fixed -translate-x-1/2"
      style="bottom: 20%"
    >
      <span class="text-xs ui-ctrl text-secondary px-3 py-1.5"> 已到底部，按 Enter 完成 </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import AnnotationPalette from './AnnotationPalette.vue'
import ScrollPreview from './ScrollPreview.vue'
import SelectionChrome from './SelectionChrome.vue'
import ShapeHandlesOverlay from './ShapeHandlesOverlay.vue'
import type { ScreenshotData, Phase } from '../composables/useTypes'
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
import { useTextDetection } from '../composables/useTextDetection'
import { useOperationScroll } from '../composables/useOperationScroll'
import { useOperationLifecycle } from '../composables/useOperationLifecycle'

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
/** 选区阶段底部快捷键提示（与 useOverlayEvents.onKeyDown 对齐） */
const selectShortcutTips = computed(() => {
  const tips = [
    { key: 'Esc', label: '取消' },
    { key: 'F', label: '全屏' },
    { key: 'C', label: '复制色值' },
  ]
  if (props.initialScreenshot.last_selection) {
    tips.splice(2, 0, { key: 'R', label: '恢复选区' })
  }
  return tips
})
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
  lastSelection: props.initialScreenshot.last_selection,
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
  onScrollCancel: () => scroll.onScrollCancel(),
  onScrollFinish: () => scroll.onScrollFinish(),
  rootEl,
})

// 滚动编排：须在 events 之后构造；events 通过闭包延迟调用 scroll 方法，无 TDZ
const scroll = useOperationScroll({
  phase,
  sel: selection.sel,
  hasSelection: selection.hasSelection,
  shapes: annotation.shapes,
  selectedShapeIndex: annotation.selectedShapeIndex,
  activeTool: annotation.activeTool,
  isDrawing: annotation.isDrawing,
  currentShape: annotation.currentShape,
  rootEl,
  reportToolbarRect: () => paletteRef.value?.reportToolbarRect(),
  doScrollCopy: actions.doScrollCopy,
  doScrollSave: actions.doScrollSave,
  doCancel: actions.doCancel,
})
const { scrollCapture, onScrollStart, onScrollFinish, onScrollSave, onScrollCancel } = scroll

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
  magnifier.setMagnifierCanvas((el as HTMLCanvasElement | null) ?? undefined)
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

function fakePointerEvent(x: number, y: number, shiftKey: boolean, buttons: number): MouseEvent {
  return {
    clientX: x,
    clientY: y,
    shiftKey,
    button: 0,
    buttons,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    preventDefault() {},
    stopPropagation() {},
  } as MouseEvent
}

/** 原生 monitor 注入：select 阶段唯一指针源（eval 异步，不能与 DOM 去重竞态）。 */
function onNativePointer(type: 'down' | 'move' | 'up', x: number, y: number, shiftKey: boolean) {
  if (phase.value !== 'select') return
  const buttons = type === 'up' ? 0 : 1
  const e = fakePointerEvent(x, y, shiftKey, buttons)
  if (type === 'down') onMouseDown(e)
  else if (type === 'move') onMouseMove(e)
  else onMouseUp(e)
}

// select 阶段禁用 DOM 指针（由 native 全权）；annotate / scroll 走 DOM
function onDomMouseDown(e: MouseEvent) {
  if (phase.value === 'select') return
  onMouseDown(e)
}
function onDomMouseMove(e: MouseEvent) {
  if (phase.value === 'select') return
  onMouseMove(e)
}
function onDomMouseUp(e: MouseEvent) {
  if (phase.value === 'select') return
  onMouseUp(e)
}

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

// 悬浮到选区控制点即显示放大窗（不出十字线）。位置与内容都由 useMagnifier
// 根据 hoveredHandle 锚定控制点（固定画面，不跟随指针在控制点内的微动）。
function onHandleEnter(id: string) {
  hoveredHandle.value = id
}

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

useOperationLifecycle({
  initialScreenshot: props.initialScreenshot,
  rootEl,
  annotateCanvas,
  phase,
  crossX,
  crossY,
  hoverWindow: selection.hoverWindow,
  findWindowAt: selection.findWindowAt,
  updateMagnifier: magnifier.updateMagnifier,
  loadPickerImage: magnifier.loadPickerImage,
  disposeMagnifier: magnifier.dispose,
  onKeyDown: events.onKeyDown,
  onNativePointer,
  redraw: drawing.redraw,
  selectedShape: annotation.selectedShape,
  shapes: annotation.shapes,
  selectedShapeIndex: annotation.selectedShapeIndex,
  annotBlurAmount: annotation.annotBlurAmount,
  annotFontSize: annotation.annotFontSize,
  annotLineWidth: annotation.annotLineWidth,
  annotBlurMode: annotation.annotBlurMode,
  activeTool: annotation.activeTool,
  textInput: textInputComposable.textInput,
  autoResizeTextInput: textInputComposable.autoResizeTextInput,
  detectText: textDetection.detect,
  textRegions: textDetection.textRegions,
})
</script>
