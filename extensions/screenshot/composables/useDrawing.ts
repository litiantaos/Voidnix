import type { Ref } from 'vue'
import type { Shape, Sel, TextRegion } from './useTypes'
import { contrastInk, textBgHPad, textBgRadius } from './useTypes'

/// 字体度量缓存（画布文字与 CSS 行盒对齐用）
const fontMetricCache = new Map<string, { ascent: number; descent: number }>()
const CJK_CHAR_RE = /[\u2E80-\u9FFF\uF900-\uFAFF\uFF01-\uFF60]/

function probeMetrics(
  ctx: CanvasRenderingContext2D,
  fontSize: number,
  family: string,
  sample: string,
): { ascent: number; descent: number } {
  const key = `${fontSize}|${family}`
  let m = fontMetricCache.get(key)
  if (!m) {
    ctx.font = `${fontSize}px ${family}`
    const mm = ctx.measureText(sample)
    m = {
      ascent:
        (mm as unknown as { fontBoundingBoxAscent?: number }).fontBoundingBoxAscent ??
        mm.actualBoundingBoxAscent,
      descent:
        (mm as unknown as { fontBoundingBoxDescent?: number }).fontBoundingBoxDescent ??
        mm.actualBoundingBoxDescent,
    }
    fontMetricCache.set(key, m)
  }
  return m
}

/// CSS 行盒基线偏移（行盒顶 → 基线）：max((lineH + ascent - descent) / 2) over 行内字体。
/// SF 支柱恒在；行含 CJK 时计入 PingFang 回退的贡献。
export function lineBaselineOffset(
  ctx: CanvasRenderingContext2D,
  fontSize: number,
  line: string,
  lineH: number,
): number {
  const sf = probeMetrics(ctx, fontSize, '-apple-system', 'Äg')
  let above = (lineH + sf.ascent - sf.descent) / 2
  if (CJK_CHAR_RE.test(line)) {
    const cjk = probeMetrics(ctx, fontSize, "'PingFang SC'", '中')
    above = Math.max(above, (lineH + cjk.ascent - cjk.descent) / 2)
  }
  // probe 污染了 ctx.font，恢复绘制字体
  ctx.font = `${fontSize}px -apple-system, sans-serif`
  return above
}

export function useDrawing(options: {
  annotateCanvas: Ref<HTMLCanvasElement | undefined>
  dpr: Ref<number>
  sel: Ref<Sel>
  bgImage: Ref<HTMLImageElement | null>
  shapes: Ref<Shape[]>
  // 屏幕上文字由 DOM 层呈现（与编辑态同管线，零位移）；导出前临时置 true 烧录进 canvas
  textOnCanvas: Ref<boolean>
  textRegions: Ref<TextRegion[]>
}) {
  function redraw(preview?: Shape | null) {
    const canvas = options.annotateCanvas.value
    if (!canvas) return
    const ctx = canvas.getContext('2d')!
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    ctx.save()
    ctx.scale(options.dpr.value, options.dpr.value)
    for (const shape of options.shapes.value) {
      if (shape.type === 'text' && !options.textOnCanvas.value) continue
      drawShape(ctx, shape)
    }
    if (preview) drawShape(ctx, preview)
    ctx.restore()
  }

  function drawShape(ctx: CanvasRenderingContext2D, shape: Shape) {
    const {
      type,
      x1,
      y1,
      x2,
      y2,
      color,
      lineWidth,
      text,
      textLines,
      textWidth,
      textBg,
      cornerRadius: cr,
      rotation,
    } = shape
    ctx.strokeStyle = color
    ctx.fillStyle = color
    ctx.lineWidth = lineWidth
    ctx.lineCap = 'round'
    ctx.lineJoin = 'round'

    const needRotation = type === 'rect' && (rotation ?? 0) !== 0
    if (needRotation) {
      const ccx = (x1 + x2) / 2,
        ccy = (y1 + y2) / 2
      ctx.save()
      ctx.translate(ccx, ccy)
      ctx.rotate(rotation!)
      ctx.translate(-ccx, -ccy)
    }

    if (type === 'rect') {
      const rx = Math.min(x1, x2),
        ry = Math.min(y1, y2)
      const rw = Math.abs(x2 - x1),
        rh = Math.abs(y2 - y1)
      const r = Math.min(cr ?? 0, rw / 2, rh / 2)
      if (r > 0) {
        ctx.beginPath()
        ctx.moveTo(rx + r, ry)
        ctx.lineTo(rx + rw - r, ry)
        ctx.arcTo(rx + rw, ry, rx + rw, ry + r, r)
        ctx.lineTo(rx + rw, ry + rh - r)
        ctx.arcTo(rx + rw, ry + rh, rx + rw - r, ry + rh, r)
        ctx.lineTo(rx + r, ry + rh)
        ctx.arcTo(rx, ry + rh, rx, ry + rh - r, r)
        ctx.lineTo(rx, ry + r)
        ctx.arcTo(rx, ry, rx + r, ry, r)
        ctx.closePath()
        ctx.stroke()
      } else {
        ctx.strokeRect(rx, ry, rw, rh)
      }
    } else if (type === 'line') {
      ctx.beginPath()
      ctx.moveTo(x1, y1)
      ctx.lineTo(x2, y2)
      ctx.stroke()
    } else if (type === 'arrow') {
      drawArrow(ctx, x1, y1, x2, y2, lineWidth)
    } else if (type === 'text' && (text || textLines)) {
      const fontSize = shape.fontSize ?? Math.max(14, lineWidth * 6)
      const lineH = Math.round(fontSize * 1.3)
      const font = `${fontSize}px -apple-system, sans-serif`
      ctx.font = font
      // 逐行计算 CSS 行盒基线偏移（同 max 公式）后用 alphabetic 直接放基线，
      // 与编辑态 textarea 的行盒模型一致，消除提交瞬间的位置抖动。
      // 中文行由 PingFang 回退渲染，其行盒贡献高于 SF 支柱，需计入 max。
      ctx.textBaseline = 'alphabetic'
      const lines = textLines ?? (text ? text.split('\n') : [])
      // 底色模式：整块圆角底（宽度 = max(换行盒宽, 实际最大行宽) + 内边距——
      // 自适应时换行盒即内容宽（贴合），手动调宽后保留手动宽度），文字换亮度对比色
      if (textBg && lines.length > 0) {
        const padX = textBgHPad(fontSize)
        let maxLineW = 0
        for (const line of lines) maxLineW = Math.max(maxLineW, ctx.measureText(line).width)
        const boxW = Math.max(textWidth ?? 0, maxLineW) + padX * 2
        ctx.fillStyle = color
        ctx.beginPath()
        ctx.roundRect(x1 - padX, y1, boxW, lineH * lines.length, textBgRadius(fontSize, lineH))
        ctx.fill()
        ctx.fillStyle = contrastInk(color)
      }
      lines.forEach((line, i) => {
        if (!line) return
        const baseline = lineBaselineOffset(ctx, fontSize, line, lineH)
        // baselineAdjust：提交时实测的编辑态基线补偿（DOM 与 canvas 度量口径差）
        const dy = shape.baselineAdjust ?? 0
        ctx.fillText(line, x1, y1 + baseline + dy + i * lineH)
      })
    } else if (type === 'blur') {
      const bw = Math.abs(x2 - x1),
        bh = Math.abs(y2 - y1)
      const bx = Math.min(x1, x2),
        by = Math.min(y1, y2)
      if (bw < 2 || bh < 2) return

      if (!options.bgImage.value) {
        ctx.save()
        ctx.fillStyle = 'rgba(128, 128, 128, 0.5)'
        ctx.fillRect(bx, by, bw, bh)
        ctx.restore()
        return
      }

      const mode = shape.blurMode ?? 'selection'

      // 文本模糊：先求出与本形状相交的文本行矩形（canvas 坐标）。
      // 若无相交（或检测尚未完成 / 无文本），直接跳过绘制：露出背景。
      let clipRects: { x: number; y: number; w: number; h: number }[] | null = null
      if (mode === 'text') {
        clipRects = []
        for (const r of options.textRegions.value) {
          const rx = r.x - options.sel.value.x
          const ry = r.y - options.sel.value.y
          const ix1 = Math.max(bx, rx)
          const iy1 = Math.max(by, ry)
          const ix2 = Math.min(bx + bw, rx + r.w)
          const iy2 = Math.min(by + bh, ry + r.h)
          if (ix2 > ix1 && iy2 > iy1) {
            clipRects.push({ x: ix1, y: iy1, w: ix2 - ix1, h: iy2 - iy1 })
          }
        }
        if (clipRects.length === 0) return
      }

      const amount = shape.blurAmount ?? 15
      const radius = Math.max(1, Math.min(100, Math.round(amount * options.dpr.value)))

      const srcX = (options.sel.value.x + bx) * options.dpr.value
      const srcY = (options.sel.value.y + by) * options.dpr.value
      const srcW = Math.max(1, Math.floor(bw * options.dpr.value))
      const srcH = Math.max(1, Math.floor(bh * options.dpr.value))

      const off = document.createElement('canvas')
      off.width = srcW
      off.height = srcH
      const offCtx = off.getContext('2d')!
      offCtx.drawImage(options.bgImage.value, srcX, srcY, srcW, srcH, 0, 0, srcW, srcH)

      const imgData = offCtx.getImageData(0, 0, srcW, srcH)
      stackBlur(imgData.data, srcW, srcH, radius)
      offCtx.putImageData(imgData, 0, 0)

      ctx.save()
      ctx.beginPath()
      if (clipRects) {
        for (const r of clipRects) ctx.rect(r.x, r.y, r.w, r.h)
      } else {
        ctx.rect(bx, by, bw, bh)
      }
      ctx.clip()
      ctx.drawImage(off, 0, 0, off.width, off.height, bx, by, bw, bh)
      ctx.restore()
    }

    if (needRotation) ctx.restore()
  }

  function drawArrow(
    ctx: CanvasRenderingContext2D,
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    lineWidth: number,
  ) {
    const angle = Math.atan2(y2 - y1, x2 - x1)
    const headLen = Math.max(12, lineWidth * 4),
      headAngle = 0.42
    const wing1X = x2 - headLen * Math.cos(angle - headAngle)
    const wing1Y = y2 - headLen * Math.sin(angle - headAngle)
    const wing2X = x2 - headLen * Math.cos(angle + headAngle)
    const wing2Y = y2 - headLen * Math.sin(angle + headAngle)
    const baseX = (wing1X + wing2X) / 2,
      baseY = (wing1Y + wing2Y) / 2
    ctx.beginPath()
    ctx.moveTo(x1, y1)
    ctx.lineTo(baseX, baseY)
    ctx.stroke()
    ctx.beginPath()
    ctx.moveTo(x2, y2)
    ctx.lineTo(wing1X, wing1Y)
    ctx.lineTo(wing2X, wing2Y)
    ctx.closePath()
    ctx.fill()
  }

  function stackBlur(data: Uint8ClampedArray, w: number, h: number, radius: number) {
    if (radius < 1) return
    const r = Math.min(radius, 254)
    const size = r * 2 + 1
    const tmp = new Uint8ClampedArray(data.length)

    function boxBlurH(src: Uint8ClampedArray, dst: Uint8ClampedArray) {
      for (let y = 0; y < h; y++) {
        let rs = 0,
          gs = 0,
          bs = 0,
          as = 0
        const row = y * w * 4
        for (let k = -r; k <= r; k++) {
          const i = row + Math.min(Math.max(k, 0), w - 1) * 4
          rs += src[i]
          gs += src[i + 1]
          bs += src[i + 2]
          as += src[i + 3]
        }
        for (let x = 0; x < w; x++) {
          const di = row + x * 4
          dst[di] = rs / size
          dst[di + 1] = gs / size
          dst[di + 2] = bs / size
          dst[di + 3] = as / size
          const rm = row + Math.min(Math.max(x - r, 0), w - 1) * 4
          const rp = row + Math.min(x + r + 1, w - 1) * 4
          rs += src[rp] - src[rm]
          gs += src[rp + 1] - src[rm + 1]
          bs += src[rp + 2] - src[rm + 2]
          as += src[rp + 3] - src[rm + 3]
        }
      }
    }

    function boxBlurV(src: Uint8ClampedArray, dst: Uint8ClampedArray) {
      for (let x = 0; x < w; x++) {
        let rs = 0,
          gs = 0,
          bs = 0,
          as = 0
        for (let k = -r; k <= r; k++) {
          const i = Math.min(Math.max(k, 0), h - 1) * w * 4 + x * 4
          rs += src[i]
          gs += src[i + 1]
          bs += src[i + 2]
          as += src[i + 3]
        }
        for (let y = 0; y < h; y++) {
          const di = y * w * 4 + x * 4
          dst[di] = rs / size
          dst[di + 1] = gs / size
          dst[di + 2] = bs / size
          dst[di + 3] = as / size
          const rm = Math.min(Math.max(y - r, 0), h - 1) * w * 4 + x * 4
          const rp = Math.min(y + r + 1, h - 1) * w * 4 + x * 4
          rs += src[rp] - src[rm]
          gs += src[rp + 1] - src[rm + 1]
          bs += src[rp + 2] - src[rm + 2]
          as += src[rp + 3] - src[rm + 3]
        }
      }
    }

    for (let pass = 0; pass < 3; pass++) {
      boxBlurH(data, tmp)
      boxBlurV(tmp, data)
    }
  }

  return {
    redraw,
    drawShape,
    drawArrow,
    stackBlur,
  }
}
