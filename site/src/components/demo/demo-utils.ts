// ═══════════════════════════════════════════════
//  常量 + 数学工具（零 DOM 依赖，被 scenes / player 共用）
// ═══════════════════════════════════════════════

export const FPS = 30

export interface Segment {
  id: string
  dur: number
  kbd: string[] | null
  cap: string
}

// ── 段定义（每段独立时长）──
export const SEGMENTS: Segment[] = [
  { id: 'search', dur: 160, kbd: ['⌥', 'Space'], cap: '全局搜索：应用、文件、扩展' },
  { id: 'clipboard', dur: 190, kbd: ['⌥', 'C'], cap: '扩展模式：剪贴板历史' },
  { id: 'agent', dur: 200, kbd: ['⌥', 'A'], cap: 'Agent：自然语言驱动工具' },
  { id: 'shot', dur: 350, kbd: ['⌥', 'S'], cap: '截屏：标注 + 滚动截屏 + OCR' },
  { id: 'snap', dur: 160, kbd: null, cap: '窗口管理：鼠标顶部触发分屏' },
  { id: 'finder', dur: 160, kbd: ['⌥', 'F'], cap: '访达工具：快捷键操作 Finder' },
]

// 累计偏移（capture / all 模式用）
export const SEG_OFFSETS: number[] = []
{
  let a = 0
  for (const s of SEGMENTS) {
    SEG_OFFSETS.push(a)
    a += s.dur
  }
}
export const TOTAL_DUR = SEG_OFFSETS[SEG_OFFSETS.length - 1] + SEGMENTS[SEGMENTS.length - 1].dur

// ── 统一入场节奏（段内局部帧号）──
export const KBD = 6
export const KBD_END = 26
export const APP = 24
export const APP_END = 52

// 每段消失开始帧
export const SEG_DIS: Record<string, number> = {
  search: 132,
  clipboard: 116,
  agent: 168,
  shot: 312,
  snap: 132,
  finder: 132,
}

// ── 段上下文（renderFrame 每段切换时设置）──
export interface SegCtx {
  dur: number
  dis: number
}

// ── 数学工具 ──
export const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v))
export const lerp = (a: number, b: number, t: number): number => a + (b - a) * t
export const easeOut = (t: number): number => 1 - Math.pow(1 - t, 3)
export const easeInOut = (t: number): number =>
  t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2

export function spring(
  f: number,
  start: number,
  stiffness = 120,
  damping = 18,
  mass = 0.6,
): number {
  const t = Math.max(0, (f - start) / FPS)
  if (t <= 0) return 0
  const w0 = Math.sqrt(stiffness / mass)
  const z = damping / (2 * Math.sqrt(stiffness * mass))
  const wd = w0 * Math.sqrt(Math.max(0, 1 - z * z))
  return 1 - Math.exp(-z * w0 * t) * Math.cos(wd * t)
}

export function typeSlice(f: number, start: number, end: number, text: string): string {
  const p = clamp((f - start) / (end - start), 0, 1)
  return text.substring(0, Math.ceil(p * text.length))
}

/// 全局帧 → (段序号, 段内帧)，capture / all 模式共用
export function globalToSeg(globalFrame: number): { segIdx: number; lf: number } {
  const tf = ((globalFrame % TOTAL_DUR) + TOTAL_DUR) % TOTAL_DUR
  let segIdx = 0
  for (let i = 0; i < SEGMENTS.length; i++) {
    if (tf < SEG_OFFSETS[i] + SEGMENTS[i].dur) {
      segIdx = i
      break
    }
  }
  return { segIdx, lf: tf - SEG_OFFSETS[segIdx] }
}
