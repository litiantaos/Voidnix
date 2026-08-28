import type { Shape } from './useTypes'
import { textBgHPad } from './useTypes'

const HIT = 8

/** 判断画布坐标是否命中形状（边/线/文本框，含旋转矩形）。 */
export function hitTestShape(shape: Shape, px: number, py: number): boolean {
  const { type, x1, y1, x2, y2, textWidth } = shape

  if (type === 'rect') {
    let lpx = px,
      lpy = py
    const r = shape.rotation ?? 0
    if (r !== 0) {
      const ccx = (x1 + x2) / 2,
        ccy = (y1 + y2) / 2
      const dx = px - ccx,
        dy = py - ccy
      const cos = Math.cos(-r),
        sin = Math.sin(-r)
      lpx = ccx + dx * cos - dy * sin
      lpy = ccy + dx * sin + dy * cos
    }
    const lx = Math.min(x1, x2),
      rx = Math.max(x1, x2)
    const ty = Math.min(y1, y2),
      by = Math.max(y1, y2)
    const onH = lpx >= lx - HIT && lpx <= rx + HIT
    const onV = lpy >= ty - HIT && lpy <= by + HIT
    return (
      (onH && (Math.abs(lpy - ty) < HIT || Math.abs(lpy - by) < HIT)) ||
      (onV && (Math.abs(lpx - lx) < HIT || Math.abs(lpx - rx) < HIT))
    )
  }
  if (type === 'blur') {
    const lx = Math.min(x1, x2),
      rx = Math.max(x1, x2),
      ty = Math.min(y1, y2),
      by = Math.max(y1, y2)
    return px >= lx && px <= rx && py >= ty && py <= by
  }
  if (type === 'line' || type === 'arrow') {
    const dx = x2 - x1,
      dy = y2 - y1,
      len2 = dx * dx + dy * dy
    if (len2 === 0) return Math.hypot(px - x1, py - y1) < HIT
    const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / len2))
    return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy)) < HIT
  }
  if (type === 'text') {
    const w = textWidth ?? 160
    const fontSize = shape.fontSize ?? Math.max(14, shape.lineWidth * 6)
    const padX = shape.textBg ? textBgHPad(fontSize) : 0
    const lines = shape.textLines ?? (shape.text ? shape.text.split('\n') : [''])
    const lineH = Math.round(fontSize * 1.3)
    const h = lineH * lines.length
    return (
      px >= x1 - HIT - padX && px <= x1 + w + HIT + padX && py >= y1 - HIT && py <= y1 + h + HIT
    )
  }
  return false
}

/** 自上而下找首个命中的形状下标；无命中返回 -1。 */
export function findShapeAt(shapes: Shape[], canvasX: number, canvasY: number): number {
  for (let i = shapes.length - 1; i >= 0; i -= 1) {
    if (hitTestShape(shapes[i], canvasX, canvasY)) return i
  }
  return -1
}
