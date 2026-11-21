import { describe, expect, it } from 'vitest'
import {
  clampUs,
  deleteVirtualRange,
  applySourceCuts,
  formatDuration,
  formatTimecode,
  frameToUs,
  framesPerSecond,
  isValidRange,
  locateVirtual,
  quantizeUs,
  segStartUs,
  segVirtualUs,
  splitAtVirtual,
  subtractCuts,
  timelineTotalUs,
  usToFrame,
  virtualOffsetToSourceUs,
  type TimelineSegment,
} from './clip-logic'

const FPS_30 = { num: 30, den: 1 }
const FPS_2997 = { num: 30000, den: 1001 }

/** 便捷构造：30fps，300 帧 = 10s 的段。 */
const seg = (over: Partial<TimelineSegment> = {}): TimelineSegment => ({
  id: `s${JSON.stringify(over).length}${over.inFrame ?? 0}-${over.outFrame ?? 0}`,
  sourceIndex: 0,
  inFrame: 0,
  outFrame: 300,
  fps: FPS_30,
  speed: 1,
  ...over,
})

describe('帧网格', () => {
  it('整数帧率往返稳定', () => {
    for (let frame = 0; frame < 600; frame += 1) {
      expect(usToFrame(frameToUs(frame, FPS_30), FPS_30)).toBe(frame)
    }
  })

  it('29.97 帧率往返稳定（长素材不漂移）', () => {
    for (const frame of [0, 1, 29, 30, 300, 1000, 10_000, 100_000]) {
      expect(usToFrame(frameToUs(frame, FPS_2997), FPS_2997)).toBe(frame)
    }
  })

  it('单帧时长按有理数换算', () => {
    expect(frameToUs(1, FPS_30)).toBe(33333)
    expect(frameToUs(1, FPS_2997)).toBe(33367)
  })

  it('打点量化到最近帧', () => {
    expect(quantizeUs(20_000, FPS_30)).toBe(frameToUs(1, FPS_30))
    expect(quantizeUs(10_000, FPS_30)).toBe(0)
  })

  it('时间码按帧网格取帧位', () => {
    expect(formatTimecode(0, FPS_30)).toBe('00:00:00:00')
    expect(formatTimecode(1_500_000, FPS_30)).toBe('00:00:01:15')
    expect(formatTimecode(61_000_000, FPS_30)).toBe('00:01:01:00')
  })

  it('时长格式', () => {
    expect(formatDuration(12_340_000)).toBe('12.34s')
    expect(formatDuration(72_500_000)).toBe('1:12.50')
  })

  it('clamp 到素材边界', () => {
    expect(clampUs(-50, 1_000_000)).toBe(0)
    expect(clampUs(2_000_000, 1_000_000)).toBe(1_000_000)
  })
})

describe('帧率展示', () => {
  it('非整数帧率取整', () => {
    expect(framesPerSecond(FPS_2997)).toBe(30)
    expect(framesPerSecond(FPS_30)).toBe(30)
  })
})

describe('时间线段：时长与定位', () => {
  it('虚拟时长 = 源时长 / 倍速', () => {
    expect(segVirtualUs(seg())).toBe(10_000_000)
    expect(segVirtualUs(seg({ speed: 2 }))).toBe(5_000_000)
    expect(segVirtualUs(seg({ speed: 0.5 }))).toBe(20_000_000)
  })

  it('成片总长 = 各段虚拟时长之和', () => {
    expect(timelineTotalUs([seg(), seg({ speed: 2 })])).toBe(15_000_000)
  })

  it('段起点累积定位', () => {
    const segs = [seg(), seg({ speed: 2 })]
    expect(segStartUs(segs, 0)).toBe(0)
    expect(segStartUs(segs, 1)).toBe(10_000_000)
  })

  it('locateVirtual：命中段与偏移，越界钳制', () => {
    const segs = [seg(), seg({ speed: 2 })] // [0,10s) [10s,15s)
    expect(locateVirtual(segs, 0)).toEqual({ index: 0, offsetUs: 0 })
    expect(locateVirtual(segs, 10_000_000)).toEqual({ index: 1, offsetUs: 0 })
    expect(locateVirtual(segs, 12_500_000)).toEqual({ index: 1, offsetUs: 2_500_000 })
    expect(locateVirtual(segs, 999_999_999)).toEqual({ index: 1, offsetUs: 5_000_000 })
    expect(locateVirtual([], 0)).toBeNull()
  })

  it('段内虚拟偏移 → 源微秒（帧网格量化，倍速换算）', () => {
    const s = seg({ speed: 2 })
    expect(virtualOffsetToSourceUs(s, 0)).toBe(0)
    expect(virtualOffsetToSourceUs(s, 1_000_000)).toBe(2_000_000) // 2x：虚拟 1s = 源 2s
    // 钳到段内
    expect(virtualOffsetToSourceUs(s, 99_000_000)).toBe(10_000_000)
  })
})

describe('切分', () => {
  it('段中一刀分为两段，边界处不动', () => {
    const segs = [seg()]
    const split = splitAtVirtual(segs, 5_000_000)
    expect(split).toHaveLength(2)
    expect(split[0].outFrame).toBe(150)
    expect(split[1].inFrame).toBe(150)
    expect(splitAtVirtual(segs, 0)).toBe(segs) // 边界原样
    expect(splitAtVirtual(segs, 10_000_000)).toBe(segs)
  })

  it('倍速段的刀口按源网格换算', () => {
    const split = splitAtVirtual([seg({ speed: 2 })], 2_500_000) // 虚拟 2.5s = 源 5s
    expect(split[0].outFrame).toBe(150)
  })

  it('入出点至少 1 帧才有效', () => {
    expect(isValidRange(0, 0)).toBe(false)
    expect(isValidRange(10, 11)).toBe(true)
  })
})

describe('删除区间（涟漪）', () => {
  it('删段中一段：一段变两段，总长缩短', () => {
    const out = deleteVirtualRange([seg()], 3_000_000, 5_000_000)
    expect(out).toHaveLength(2)
    expect(out[0].outFrame).toBe(90)
    expect(out[1].inFrame).toBe(150)
    expect(timelineTotalUs(out)).toBe(8_000_000)
  })

  it('删整段：直接消失，后段前移', () => {
    const segs = [seg(), seg({ inFrame: 300, outFrame: 600 }), seg({ sourceIndex: 1 })]
    const out = deleteVirtualRange(segs, 10_000_000, 20_000_000)
    expect(out).toHaveLength(2)
    expect(out[0]).toBe(segs[0])
    expect(out[1]).toBe(segs[2])
  })

  it('跨段删除：切段尾 + 断头，中间全消', () => {
    const a = seg() // [0,10s)
    const b = seg({ inFrame: 300, outFrame: 600 }) // [10s,20s)
    const c = seg({ sourceIndex: 1 }) // [20s,30s)
    const out = deleteVirtualRange([a, b, c], 8_000_000, 25_000_000)
    expect(out).toHaveLength(2)
    expect(out[0].outFrame).toBe(240) // a 保留 [0,8s)
    expect(out[1].sourceIndex).toBe(1)
    expect(out[1].inFrame).toBe(150) // c 保留 [5s,10s)
  })

  it('倍速段上的删除按源网格换算', () => {
    // 2x 段：虚拟 [0,5s) = 源 [0,10s)。删虚拟 [1s,2s) = 源 [2s,4s)
    const out = deleteVirtualRange([seg({ speed: 2 })], 1_000_000, 2_000_000)
    expect(out).toHaveLength(2)
    expect(out[0].outFrame).toBe(60)
    expect(out[1].inFrame).toBe(120)
  })

  it('区间与段不相交：内容原样', () => {
    const segs = [seg()]
    expect(deleteVirtualRange(segs, -1_000_000, 0)).toEqual(segs)
    expect(deleteVirtualRange(segs, 10_000_000, 12_000_000)).toEqual(segs)
  })
})

describe('源级废段应用（去停顿）', () => {
  it('段与废段相交：减出子段继承倍速', () => {
    const out = applySourceCuts([seg({ speed: 2 })], 0, [{ startUs: 2_000_000, endUs: 4_000_000 }])
    expect(out).toHaveLength(2)
    expect(out[0].outFrame).toBe(60)
    expect(out[1].inFrame).toBe(120)
    expect(out.every((s) => s.speed === 2)).toBe(true)
  })

  it('废段只影响目标源', () => {
    const other = seg({ sourceIndex: 1 })
    const out = applySourceCuts([seg(), other], 0, [{ startUs: 0, endUs: 10_000_000 }])
    expect(out).toEqual([other])
  })

  it('废段跨多段实例：逐段减除', () => {
    const a = seg() // 源 [0,10s)
    const b = seg({ inFrame: 300, outFrame: 600 }) // 源 [10s,20s)
    const out = applySourceCuts([a, b], 0, [{ startUs: 5_000_000, endUs: 15_000_000 }])
    expect(out).toHaveLength(2)
    expect(out[0]).toMatchObject({ inFrame: 0, outFrame: 150 })
    expect(out[1]).toMatchObject({ inFrame: 450, outFrame: 600 })
  })
})

describe('区间减法（subtractCuts）', () => {
  it('单个中段删减', () => {
    expect(subtractCuts(10_000_000, [{ startUs: 2_000_000, endUs: 4_000_000 }])).toEqual([
      { startUs: 0, endUs: 2_000_000 },
      { startUs: 4_000_000, endUs: 10_000_000 },
    ])
  })

  it('乱序 + 重叠删减收束', () => {
    expect(
      subtractCuts(10_000_000, [
        { startUs: 6_000_000, endUs: 8_000_000 },
        { startUs: 1_000_000, endUs: 3_000_000 },
        { startUs: 2_000_000, endUs: 7_000_000 },
      ]),
    ).toEqual([
      { startUs: 0, endUs: 1_000_000 },
      { startUs: 8_000_000, endUs: 10_000_000 },
    ])
  })

  it('全删返回空', () => {
    expect(subtractCuts(5_000_000, [{ startUs: 0, endUs: 5_000_000 }])).toEqual([])
  })
})
