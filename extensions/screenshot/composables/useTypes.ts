export interface WindowRect {
  x: number
  y: number
  w: number
  h: number
  owner: string
}

export interface ScreenshotData {
  width: number
  height: number
  scale: number
  mouse_x: number
  mouse_y: number
  windows: WindowRect[]
  last_selection: Sel | null
}

export interface Sel {
  x: number
  y: number
  w: number
  h: number
}

export type Tool = 'rect' | 'line' | 'arrow' | 'text' | 'blur' | null
export type Phase = 'select' | 'annotate' | 'scroll'
export type BlurMode = 'selection' | 'text'

export interface Shape {
  type: Tool
  x1: number
  y1: number
  x2: number
  y2: number
  color: string
  lineWidth: number
  text?: string
  textLines?: string[]
  textWidth?: number
  fontSize?: number
  textBg?: boolean
  /// 提交时实测的基线补偿（px）：DOM 行盒度量与 canvas measureText 存在约半像素口径差
  baselineAdjust?: number
  cornerRadius?: number
  rotation?: number
  blurAmount?: number
  blurMode?: BlurMode
}

export interface ShapeHandle {
  id: string
  style: Record<string, string>
}

/// 屏幕坐标系下检测出的文本行边界（CSS 像素，左上原点）。
export interface TextRegion {
  x: number
  y: number
  w: number
  h: number
}

export const MAGNIFIER_SIZE = 120
export const MAGNIFIER_ZOOM = 4
export const MAGNIFIER_OFFSET = 20
export const DRAG_THRESHOLD = 4
/// 文本框初始默认宽 / 拖动最小宽
export const TEXT_MIN_WIDTH = 40
/// 自适应宽度的下限（短文本时框贴合内容，可低于初始宽）
export const TEXT_AUTO_MIN_WIDTH = 16
export const TEXT_DRAG_PAD = 4
export const PALETTE_H = 44
export const PALETTE_GAP = 8

/// 标签底色的水平内边距（随字号缩放）
export function textBgHPad(fontSize: number): number {
  return Math.round(fontSize * 0.35)
}

/// 标签底色圆角（不超过半行高）
export function textBgRadius(fontSize: number, lineH: number): number {
  return Math.min(lineH / 2, Math.max(4, Math.round(fontSize * 0.3)))
}

/// 按相对亮度选底色标签的文字对比色（亮底黑字 / 暗底白字）
export function contrastInk(bg: string): string {
  const hex = bg.replace('#', '')
  const v =
    hex.length === 3
      ? hex
          .split('')
          .map((c) => c + c)
          .join('')
      : hex
  const r = parseInt(v.slice(0, 2), 16) / 255
  const g = parseInt(v.slice(2, 4), 16) / 255
  const b = parseInt(v.slice(4, 6), 16) / 255
  const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
  return lum > 0.6 ? '#000000' : '#ffffff'
}

/// 控制点在屏幕坐标系（CSS 像素，左上原点）下的绝对坐标。
export function handleAbsolutePos(id: string, sel: Sel): { x: number; y: number } {
  const { x, y, w, h } = sel
  const map: Record<string, [number, number]> = {
    nw: [x, y],
    n: [x + w / 2, y],
    ne: [x + w, y],
    w: [x, y + h / 2],
    e: [x + w, y + h / 2],
    sw: [x, y + h],
    s: [x + w / 2, y + h],
    se: [x + w, y + h],
  }
  const p = map[id]
  return p ? { x: p[0], y: p[1] } : { x, y }
}
