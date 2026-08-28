import { ref, computed, nextTick, watch, type Ref } from 'vue'
import type { Shape, Sel } from './useTypes'
import {
  TEXT_AUTO_MIN_WIDTH,
  TEXT_MIN_WIDTH,
  TEXT_DRAG_PAD,
  contrastInk,
  textBgHPad,
  textBgRadius,
} from './useTypes'
import { wrapText } from './wrapText'
import { lineBaselineOffset } from './useDrawing'

/// 提交时实测「编辑态字形原点」与「画布将绘制字形原点」的基线差（DOM 行盒度量与
/// canvas measureText 度量存在约半像素口径差），作为 baselineAdjust 存入 shape，
/// 使提交态精确落在编辑态位置。
function measureBaselineAdjust(
  el: HTMLTextAreaElement,
  sel: Sel,
  y1: number,
  fontSize: number,
  firstLine: string,
): number {
  try {
    const cs = getComputedStyle(el)
    const er = el.getBoundingClientRect()
    const mirror = document.createElement('div')
    mirror.textContent = firstLine
    Object.assign(mirror.style, {
      position: 'fixed',
      left: '0px',
      top: '0px',
      visibility: 'hidden',
      whiteSpace: 'pre',
      fontFamily: cs.fontFamily,
      fontSize: cs.fontSize,
      lineHeight: cs.lineHeight,
    })
    document.body.appendChild(mirror)
    const range = document.createRange()
    range.selectNodeContents(mirror)
    const rects = range.getClientRects()
    const glyph = rects.length ? rects[0] : null
    mirror.remove()
    if (!glyph) return 0
    const pt = parseFloat(cs.paddingTop) || 0
    // 编辑态字形顶（页内绝对坐标）= textarea 内容盒顶 + 镜像内相对偏移
    const edTop = er.top + pt + glyph.top

    const scratch = document.createElement('canvas').getContext('2d')!
    const baseline = lineBaselineOffset(scratch, fontSize, firstLine, Math.round(fontSize * 1.3))
    // 画布侧字形顶 = 基线减 ascent
    scratch.font = `${fontSize}px -apple-system, sans-serif`
    const m = scratch.measureText(firstLine)
    const ascent =
      (m as unknown as { fontBoundingBoxAscent?: number }).fontBoundingBoxAscent ??
      m.actualBoundingBoxAscent
    const cvTop = sel.y + y1 + baseline - ascent
    return edTop - cvTop
  } catch {
    return 0
  }
}

export function useTextInput(options: {
  sel: Ref<Sel>
  annotColor: Ref<string>
  annotLineWidth: Ref<number>
  annotFontSize: Ref<number>
  annotTextBg: Ref<boolean>
  shapes: Ref<Shape[]>
  selectedShapeIndex: Ref<number | null>
  isHoveringSelectedShape: Ref<boolean>
  redraw: () => void
  textInputEl: Ref<HTMLTextAreaElement | undefined>
}) {
  const textInputEl = options.textInputEl
  const textInput = ref({
    visible: false,
    value: '',
    x: 0,
    y: 0,
    canvasX: 0,
    canvasY: 0,
    width: TEXT_MIN_WIDTH,
    editingIndex: null as number | null,
    /// 编辑会话内拖过宽度手柄后，自适应宽度让位（重开编辑复位）
    manualWidth: false,
  })

  const textFontSize = computed(() => options.annotFontSize.value)

  const isDraggingTextInput = ref(false)
  const textInputDragStart = ref({ mx: 0, my: 0, canvasX: 0, canvasY: 0 })
  const textInputPendingDrag = ref(false)

  function startTextInputDrag(e: MouseEvent) {
    if (!textInput.value.visible) return
    const idx = textInput.value.editingIndex
    if (idx === null) return
    textInputPendingDrag.value = true
    isDraggingTextInput.value = false
    textInputDragStart.value = {
      mx: e.clientX,
      my: e.clientY,
      canvasX: textInput.value.canvasX,
      canvasY: textInput.value.canvasY,
    }
    e.preventDefault()
  }

  function onTextInputWrapperMouseDown(e: MouseEvent) {
    e.stopPropagation()
  }

  /// 编辑中形状（新建时为占位 shape，携带当前标注参数）
  function editingShape(): Shape | undefined {
    const idx = textInput.value.editingIndex
    return idx !== null ? options.shapes.value[idx] : undefined
  }

  const textInputWrapperStyle = computed(() => {
    const s = editingShape()
    const padX = s?.textBg ? textBgHPad(s.fontSize ?? Math.max(14, s.lineWidth * 6)) : 0
    return {
      left: `${textInput.value.x - TEXT_DRAG_PAD - padX}px`,
      top: `${textInput.value.y - TEXT_DRAG_PAD}px`,
      width: `${textInput.value.width + (TEXT_DRAG_PAD + padX) * 2}px`,
      minWidth: `${TEXT_AUTO_MIN_WIDTH + (TEXT_DRAG_PAD + padX) * 2}px`,
      padding: `${TEXT_DRAG_PAD}px`,
    }
  })

  const textInputInnerStyle = computed(() => {
    const s = editingShape()
    const color = s?.color ?? options.annotColor.value
    const fs = s?.fontSize ?? Math.max(14, (s?.lineWidth ?? options.annotLineWidth.value) * 6)
    const bg = s?.textBg === true
    const padX = bg ? textBgHPad(fs) : 0
    return {
      color: bg ? contrastInk(color) : color,
      fontSize: `${fs}px`,
      lineHeight: `${Math.round(fs * 1.3)}px`,
      padding: bg ? `0 ${padX}px` : '0',
      margin: '0',
      overflow: 'hidden',
      fontFamily: '-apple-system, sans-serif',
      ...(bg
        ? {
            background: color,
            borderRadius: `${textBgRadius(fs, Math.round(fs * 1.3))}px`,
          }
        : {}),
    }
  })

  /// 外侧虚线拖动框：圆角统一按「底色块圆角 + 4px 间隙」随字号缩放，
  /// 底色模式与色块同心，纯文本同公式——两模式切换圆角不跳变
  const textInputFrameStyle = computed(() => {
    const s = editingShape()
    const fs = s?.fontSize ?? Math.max(14, (s?.lineWidth ?? options.annotLineWidth.value) * 6)
    return {
      padding: '1px',
      border: '1px dashed var(--color-accent)',
      borderRadius: `${textBgRadius(fs, Math.round(fs * 1.3)) + TEXT_DRAG_PAD}px`,
    }
  })

  function autoResizeTextInput() {
    const el = textInputEl.value
    if (!el) return
    el.style.height = 'auto'
    el.style.height = el.scrollHeight + 'px'
  }

  const measureCanvas = document.createElement('canvas')

  /// 文字框宽度默认自适应内容：实测各行取最大宽（clamp 最小宽与选区右缘），
  /// 同步输入框与占位 shape，虚线框 / 控制点 / 底色即时跟随
  function autoWidthTextInput() {
    const idx = textInput.value.editingIndex
    if (idx === null || textInput.value.manualWidth) return
    const s = options.shapes.value[idx]
    const fs = s?.fontSize ?? options.annotFontSize.value
    const ctx = measureCanvas.getContext('2d')!
    ctx.font = `${fs}px -apple-system, sans-serif`
    // 直读 textarea 值：拼音等输入法组词期间 v-model 不更新，宽度需随组词实时变化
    const content = textInputEl.value ? textInputEl.value.value : textInput.value.value
    let maxW = 0
    for (const line of content.split('\n')) {
      maxW = Math.max(maxW, ctx.measureText(line).width)
    }
    const maxAvail = Math.max(
      TEXT_AUTO_MIN_WIDTH,
      options.sel.value.w - textInput.value.canvasX - 10,
    )
    textInput.value.width = Math.min(Math.max(Math.ceil(maxW), TEXT_AUTO_MIN_WIDTH), maxAvail)
    if (s) s.textWidth = textInput.value.width
  }

  // 宽度变化（自适应 / 拖手柄）后重算高度：换行行数变化时编辑框（含底色）始终贴合内容
  watch(
    () => textInput.value.width,
    () => nextTick(() => autoResizeTextInput()),
  )

  function onTextInputInput() {
    autoWidthTextInput()
    nextTick(() => autoResizeTextInput())
  }

  function onTextInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      cancelText()
      return
    }
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      commitText()
    }
  }

  function openTextInput(screenX: number, screenY: number, editIndex?: number) {
    const idx = editIndex ?? null
    const existing = idx !== null ? options.shapes.value[idx] : null

    // 文字原点取整：小数坐标下 DOM 与 canvas 栅格取整策略不同，提交后会偏移约 1px
    const cx = Math.round(screenX - options.sel.value.x)
    const cy = Math.round(screenY - options.sel.value.y)

    let resolvedIdx = idx
    if (resolvedIdx === null) {
      const placeholder: Shape = {
        type: 'text',
        x1: cx,
        y1: cy,
        x2: cx,
        y2: cy,
        color: options.annotColor.value,
        lineWidth: options.annotLineWidth.value,
        fontSize: options.annotFontSize.value,
        textBg: options.annotTextBg.value,
        text: '',
        textLines: [],
        textWidth: TEXT_MIN_WIDTH,
      }
      options.shapes.value.push(placeholder)
      resolvedIdx = options.shapes.value.length - 1
      options.selectedShapeIndex.value = resolvedIdx
    }

    const canvasX = existing ? existing.x1 : cx
    const canvasY = existing ? existing.y1 : cy
    textInput.value = {
      visible: true,
      value: existing?.text ?? '',
      x: options.sel.value.x + canvasX,
      y: options.sel.value.y + canvasY,
      canvasX,
      canvasY,
      width: existing?.textWidth ?? TEXT_MIN_WIDTH,
      editingIndex: resolvedIdx,
      manualWidth: false,
    }
    nextTick(() => {
      // 挂载后立即校正高度：多行文本首帧即按完整行数呈现，避免先单行再展开的闪动
      autoResizeTextInput()
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          textInputEl.value?.focus()
          if (existing && textInputEl.value) {
            const len = textInputEl.value.value.length
            textInputEl.value.setSelectionRange(len, len)
          }
          autoResizeTextInput()
        })
      })
    })
  }

  function commitText() {
    if (!textInput.value.visible) return
    const t = textInput.value.value
    const idx = textInput.value.editingIndex
    const el = textInputEl.value
    textInput.value.visible = false
    textInput.value.editingIndex = null

    if (t.trim() && idx !== null) {
      const existing = options.shapes.value[idx]
      const fontSize =
        existing?.fontSize ??
        Math.max(14, (existing?.lineWidth ?? options.annotLineWidth.value) * 6)
      const font = `${fontSize}px -apple-system, sans-serif`
      const wrappedLines = wrapText(t, textInput.value.width, font)
      const baselineAdjust = el
        ? measureBaselineAdjust(
            el,
            options.sel.value,
            textInput.value.canvasY,
            fontSize,
            t.split('\n')[0] || ' ',
          )
        : 0

      const shape: Shape = {
        type: 'text',
        x1: textInput.value.canvasX,
        y1: textInput.value.canvasY,
        x2: textInput.value.canvasX,
        y2: textInput.value.canvasY,
        color: existing?.color ?? options.annotColor.value,
        lineWidth: existing?.lineWidth ?? options.annotLineWidth.value,
        fontSize,
        textBg: existing?.textBg ?? options.annotTextBg.value,
        baselineAdjust,
        text: t,
        textLines: wrappedLines,
        textWidth: textInput.value.width,
      }
      options.shapes.value[idx] = shape
      options.selectedShapeIndex.value = null
      options.isHoveringSelectedShape.value = false
      options.redraw()
    } else if (idx !== null) {
      options.shapes.value.splice(idx, 1)
      options.selectedShapeIndex.value = null
      options.isHoveringSelectedShape.value = false
      options.redraw()
    }
  }

  function cancelText() {
    const idx = textInput.value.editingIndex
    if (idx !== null && options.shapes.value[idx]?.text === '') {
      options.shapes.value.splice(idx, 1)
      options.selectedShapeIndex.value = null
      options.isHoveringSelectedShape.value = false
      options.redraw()
    }
    textInput.value.visible = false
    textInput.value.editingIndex = null
  }

  return {
    textInput,
    textFontSize,
    isDraggingTextInput,
    textInputDragStart,
    textInputPendingDrag,
    textInputWrapperStyle,
    textInputInnerStyle,
    textInputFrameStyle,
    startTextInputDrag,
    onTextInputWrapperMouseDown,
    autoResizeTextInput,
    onTextInputInput,
    onTextInputKeydown,
    openTextInput,
    commitText,
    cancelText,
  }
}
