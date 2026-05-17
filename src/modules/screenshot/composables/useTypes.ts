// ── 类型 ──────────────────────────────────────────────────
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
export type Phase = 'select' | 'annotate'

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
  cornerRadius?: number
  rotation?: number
  blurAmount?: number
}

export interface ShapeHandle {
  id: string
  style: Record<string, string>
}

// ── 常量 ──────────────────────────────────────────────────
export const MAGNIFIER_SIZE = 120
export const MAGNIFIER_ZOOM = 4
export const MAGNIFIER_OFFSET = 20
export const DRAG_THRESHOLD = 4
export const TEXT_MIN_WIDTH = 80
export const TEXT_DRAG_PAD = 4
export const PALETTE_H = 44
export const PALETTE_GAP = 8
