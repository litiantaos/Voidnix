/**
 * 剪辑纯逻辑：帧网格、时间码、时间线段运算。
 *
 * 成片 = 时间线段列表按序拼接（虚拟时间线）。每段是「源素材的一个实例」：
 * 源帧网格入出点 + 倍速，段时长（虚拟）= 源时长 / speed。删除/切分/排序都是
 * 列表变换——涟漪前移由列表语义天然承载，无独立"轨道"状态。
 *
 * 时间基准与 Rust 侧一致：整数微秒（i64 语义）+ 有理数帧率（fpsNum/fpsDen），
 * 禁止用浮点秒参与打点/比较（长素材会累积漂移）。打点量化到帧网格，
 * 导出时按同一帧网格换算 ffmpeg 时间参数。
 */

export interface FrameRate {
  num: number
  den: number
}

/** 单帧时长（微秒，可为非整数：30000/1001 → 33366.7us）。 */
export function frameDurationUs(fps: FrameRate): number {
  if (fps.num <= 0 || fps.den <= 0) return 33333
  return (1_000_000 * fps.den) / fps.num
}

/** 微秒 → 最近帧号。 */
export function usToFrame(us: number, fps: FrameRate): number {
  return Math.max(0, Math.round(us / frameDurationUs(fps)))
}

/** 帧号 → 微秒（四舍五入到整数微秒）。 */
export function frameToUs(frame: number, fps: FrameRate): number {
  return Math.max(0, Math.round(frame * frameDurationUs(fps)))
}

/** 量化到帧网格（打点/seek 目标统一走这里）。 */
export function quantizeUs(us: number, fps: FrameRate): number {
  return frameToUs(usToFrame(us, fps), fps)
}

/** 整数帧率（用于时间码的帧位）。非整数帧率按四舍五入展示帧位。 */
export function framesPerSecond(fps: FrameRate): number {
  if (fps.num <= 0 || fps.den <= 0) return 30
  return Math.max(1, Math.round(fps.num / fps.den))
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n)
}

/** HH:MM:SS:FF 时间码（按帧网格取帧号，避免浮点秒舍入）。 */
export function formatTimecode(us: number, fps: FrameRate): string {
  const safe = Math.max(0, Math.round(us))
  const totalFrames = usToFrame(safe, fps)
  const fpsInt = framesPerSecond(fps)
  const ff = totalFrames % fpsInt
  const totalSecs = Math.floor(safe / 1_000_000)
  const ss = totalSecs % 60
  const mm = Math.floor(totalSecs / 60) % 60
  const hh = Math.floor(totalSecs / 3600)
  return `${pad2(hh)}:${pad2(mm)}:${pad2(ss)}:${pad2(ff)}`
}

/** 00:12.35 形式的时长（带两位小数秒）。 */
export function formatDuration(us: number): string {
  const safe = Math.max(0, Math.round(us))
  const totalSecs = safe / 1_000_000
  const mm = Math.floor(totalSecs / 60)
  const ss = totalSecs - mm * 60
  if (mm <= 0) return `${ss.toFixed(2)}s`
  return `${mm}:${ss.toFixed(2).padStart(5, '0')}`
}

export function clampUs(us: number, durationUs: number): number {
  return Math.min(Math.max(0, Math.round(us)), Math.max(0, durationUs))
}

/** 入/出点是否构成有效区间（至少 1 帧）。 */
export function isValidRange(inFrame: number, outFrame: number): boolean {
  return outFrame > inFrame
}

let seq = 0
export function newSegmentId(): string {
  seq += 1
  return `seg-${Date.now().toString(36)}-${seq}`
}

// ---------------------------------------------------------------------------
// 时间线段
// ---------------------------------------------------------------------------

/** 倍速预设（导出/播放共用；atempo 链可覆盖全部值）。 */
export const SPEED_PRESETS = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4] as const

/** 时间线段：源素材的一段实例。成片 = 段列表按序拼接。 */
export interface TimelineSegment {
  id: string
  /** 源在会话素材列表中的序号 */
  sourceIndex: number
  /** 源帧网格入/出点（出点不含） */
  inFrame: number
  outFrame: number
  fps: FrameRate
  /** 倍速（0.25–4）：成片时长 = 源时长 / speed */
  speed: number
}

/** 段的源侧时长（微秒）。 */
export function segSourceUs(seg: TimelineSegment): number {
  return frameToUs(seg.outFrame - seg.inFrame, seg.fps)
}

/** 段在成片（虚拟时间线）中的时长。 */
export function segVirtualUs(seg: TimelineSegment): number {
  return Math.max(1, Math.round(segSourceUs(seg) / seg.speed))
}

/** 成片总时长。 */
export function timelineTotalUs(segs: TimelineSegment[]): number {
  return segs.reduce((sum, seg) => sum + segVirtualUs(seg), 0)
}

/** 段起点在虚拟时间线上的位置。 */
export function segStartUs(segs: TimelineSegment[], index: number): number {
  let cursor = 0
  for (let i = 0; i < index && i < segs.length; i += 1) cursor += segVirtualUs(segs[i])
  return cursor
}

/** 虚拟位置 → 段序号 + 段内虚拟偏移（越界钳制到首/尾段）。空表返回 null。 */
export function locateVirtual(
  segs: TimelineSegment[],
  virtualUs: number,
): { index: number; offsetUs: number } | null {
  if (!segs.length) return null
  let cursor = 0
  for (let index = 0; index < segs.length; index += 1) {
    const len = segVirtualUs(segs[index])
    if (virtualUs < cursor + len || index === segs.length - 1) {
      return { index, offsetUs: Math.min(Math.max(0, virtualUs - cursor), len) }
    }
    cursor += len
  }
  return { index: 0, offsetUs: 0 }
}

/** 段内虚拟偏移 → 源微秒（帧网格量化，钳到段内）。 */
export function virtualOffsetToSourceUs(seg: TimelineSegment, offsetUs: number): number {
  const inUs = frameToUs(seg.inFrame, seg.fps)
  const outUs = frameToUs(seg.outFrame, seg.fps)
  const raw = inUs + offsetUs * seg.speed
  return Math.min(outUs, quantizeUs(Math.max(inUs, raw), seg.fps))
}

/** 段内虚拟偏移 → 源帧号（钳到段内）。 */
function virtualOffsetToFrame(seg: TimelineSegment, offsetUs: number): number {
  return usToFrame(virtualOffsetToSourceUs(seg, offsetUs), seg.fps)
}

/** 在虚拟位置切一刀：所在段一分为二（源帧网格对齐；刀口落段边界则原样返回）。 */
export function splitAtVirtual(segs: TimelineSegment[], virtualUs: number): TimelineSegment[] {
  const at = locateVirtual(segs, virtualUs)
  if (!at) return segs
  const seg = segs[at.index]
  const cutFrame = virtualOffsetToFrame(seg, at.offsetUs)
  if (cutFrame <= seg.inFrame || cutFrame >= seg.outFrame) return segs
  const out = [...segs]
  out.splice(
    at.index,
    1,
    { ...seg, outFrame: cutFrame },
    { ...seg, id: newSegmentId(), inFrame: cutFrame },
  )
  return out
}

/**
 * 删除虚拟区间 [startUs, endUs)：可跨段/切半段，余下部分涟漪前移（列表重写天然达成）。
 * 半段切除的刀口按源帧网格量化；切完不足 1 帧的残余段丢弃。
 */
export function deleteVirtualRange(
  segs: TimelineSegment[],
  startUs: number,
  endUs: number,
): TimelineSegment[] {
  if (endUs <= startUs || !segs.length) return segs
  const out: TimelineSegment[] = []
  let cursor = 0
  for (const seg of segs) {
    const len = segVirtualUs(seg)
    const segStart = cursor
    const segEnd = cursor + len
    cursor = segEnd
    if (segEnd <= startUs || segStart >= endUs) {
      out.push(seg) // 与删除区间不相交
      continue
    }
    const headLen = Math.min(segEnd, startUs) - segStart // 保留头部的虚拟长度
    const tailStart = Math.max(segStart, endUs) - segStart // 保留尾部的虚拟起点
    if (headLen > 0) {
      const cutFrame = virtualOffsetToFrame(seg, headLen)
      if (cutFrame > seg.inFrame) out.push({ ...seg, outFrame: cutFrame })
    }
    if (tailStart < len) {
      const cutFrame = virtualOffsetToFrame(seg, tailStart)
      if (cutFrame < seg.outFrame) {
        out.push({ ...seg, id: newSegmentId(), inFrame: cutFrame })
      }
    }
  }
  return out
}

/**
 * 对某源应用废段区间（源坐标系微秒）：该源的每段做减法，余下子段继承倍速。
 * 供「一键去停顿」等源级分析结果落到时间线（跨段自动生效）。
 */
export function applySourceCuts(
  segs: TimelineSegment[],
  sourceIndex: number,
  cutRanges: Array<{ startUs: number; endUs: number }>,
): TimelineSegment[] {
  if (!cutRanges.length || !segs.length) return segs
  const out: TimelineSegment[] = []
  for (const seg of segs) {
    if (seg.sourceIndex !== sourceIndex) {
      out.push(seg)
      continue
    }
    const inUs = frameToUs(seg.inFrame, seg.fps)
    const lenUs = segSourceUs(seg)
    // 源坐标 → 段内坐标平移后做减法
    const local = cutRanges
      .map((r) => ({ startUs: r.startUs - inUs, endUs: r.endUs - inUs }))
      .filter((r) => r.endUs > 0 && r.startUs < lenUs)
    for (const kept of subtractCuts(lenUs, local)) {
      const inFrame = seg.inFrame + usToFrame(kept.startUs, seg.fps)
      const outFrame = seg.inFrame + usToFrame(kept.endUs, seg.fps)
      if (isValidRange(inFrame, outFrame)) {
        out.push({ ...seg, id: newSegmentId(), inFrame, outFrame })
      }
    }
  }
  return out
}

/**
 * 把时间轴上的「区间删减集合」合成为保留区间。
 *
 * cuts 为待删除区间（微秒），返回保留区间列表（升序、去重叠）。
 */
export function subtractCuts(
  durationUs: number,
  cuts: Array<{ startUs: number; endUs: number }>,
): Array<{ startUs: number; endUs: number }> {
  const kept: Array<{ startUs: number; endUs: number }> = []
  let cursor = 0
  const sorted = [...cuts].sort((a, b) => a.startUs - b.startUs)
  for (const cut of sorted) {
    const start = Math.max(cursor, Math.round(cut.startUs))
    const end = Math.min(durationUs, Math.round(cut.endUs))
    if (end <= cursor) continue
    if (start > cursor) kept.push({ startUs: cursor, endUs: Math.min(start, durationUs) })
    cursor = Math.max(cursor, end)
  }
  if (cursor < durationUs) kept.push({ startUs: cursor, endUs: durationUs })
  return kept.filter((r) => r.endUs > r.startUs)
}
