import { ref, computed, nextTick, type Ref } from 'vue'
import type { Shape, Sel } from './useTypes'
import { TEXT_MIN_WIDTH, TEXT_DRAG_PAD } from './useTypes'
import { wrapText } from './wrapText'

export function useTextInput(options: {
  sel: Ref<Sel>
  annotColor: Ref<string>
  annotLineWidth: Ref<number>
  annotFontSize: Ref<number>
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
    width: 160,
    editingIndex: null as number | null,
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

  const textInputWrapperStyle = computed(() => ({
    left: `${textInput.value.x - TEXT_DRAG_PAD}px`,
    top: `${textInput.value.y - TEXT_DRAG_PAD}px`,
    width: `${textInput.value.width + TEXT_DRAG_PAD * 2}px`,
    minWidth: `${TEXT_MIN_WIDTH + TEXT_DRAG_PAD * 2}px`,
    padding: `${TEXT_DRAG_PAD}px`,
  }))

  const textInputInnerStyle = computed(() => {
    const idx = textInput.value.editingIndex
    const color =
      idx !== null
        ? (options.shapes.value[idx]?.color ?? options.annotColor.value)
        : options.annotColor.value
    const fs =
      idx !== null
        ? (options.shapes.value[idx]?.fontSize
            ?? Math.max(14, (options.shapes.value[idx]?.lineWidth ?? options.annotLineWidth.value) * 6))
        : options.annotFontSize.value
    return {
      color,
      fontSize: `${fs}px`,
      lineHeight: `${Math.round(fs * 1.3)}px`,
      padding: '0',
      margin: '0',
      overflow: 'hidden',
      fontFamily: '-apple-system, sans-serif',
    }
  })

  function autoResizeTextInput() {
    const el = textInputEl.value
    if (!el) return
    el.style.height = 'auto'
    el.style.height = el.scrollHeight + 'px'
  }

  function onTextInputInput() {
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

    let resolvedIdx = idx
    if (resolvedIdx === null) {
      const placeholder: Shape = {
        type: 'text',
        x1: screenX - options.sel.value.x,
        y1: screenY - options.sel.value.y,
        x2: screenX - options.sel.value.x,
        y2: screenY - options.sel.value.y,
        color: options.annotColor.value,
        lineWidth: options.annotLineWidth.value,
        fontSize: options.annotFontSize.value,
        text: '',
        textLines: [],
        textWidth: 160,
      }
      options.shapes.value.push(placeholder)
      resolvedIdx = options.shapes.value.length - 1
      options.selectedShapeIndex.value = resolvedIdx
    }

    const canvasX = existing ? existing.x1 : screenX - options.sel.value.x
    const canvasY = existing ? existing.y1 : screenY - options.sel.value.y
    textInput.value = {
      visible: true,
      value: existing?.text ?? '',
      x: options.sel.value.x + canvasX,
      y: options.sel.value.y + canvasY,
      canvasX,
      canvasY,
      width: existing?.textWidth ?? 160,
      editingIndex: resolvedIdx,
    }
    nextTick(() => {
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
    textInput.value.visible = false
    textInput.value.editingIndex = null

    if (t.trim() && idx !== null) {
      const existing = options.shapes.value[idx]
      const fontSize = existing?.fontSize
        ?? Math.max(14, (existing?.lineWidth ?? options.annotLineWidth.value) * 6)
      const font = `${fontSize}px -apple-system, sans-serif`
      const wrappedLines = wrapText(t, textInput.value.width, font)

      const shape: Shape = {
        type: 'text',
        x1: textInput.value.canvasX,
        y1: textInput.value.canvasY,
        x2: textInput.value.canvasX,
        y2: textInput.value.canvasY,
        color: existing?.color ?? options.annotColor.value,
        lineWidth: existing?.lineWidth ?? options.annotLineWidth.value,
        fontSize,
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
