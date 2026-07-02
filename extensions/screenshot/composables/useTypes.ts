export interface WindowRect {
  x: number
  y: number
  w: number
  h: number
  owner: string
}

export interface ScreenshotData {
  data_url: string
  width: number
  height: number
  scale: number
  mouse_x: number
  mouse_y: number
  windows: WindowRect[]
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
export const TEXT_MIN_WIDTH = 80
export const TEXT_DRAG_PAD = 4
export const PALETTE_H = 44
export const PALETTE_GAP = 8

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
