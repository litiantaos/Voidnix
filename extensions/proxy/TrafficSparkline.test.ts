import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import TrafficSparkline from './TrafficSparkline.vue'

/** 曲线几何契约：固定 60s 窗口、左侧 0 垫底、峰值归一化、面积闭合到底边。 */
describe('TrafficSparkline', () => {
  const lineOf = (w: ReturnType<typeof mount>) =>
    w.findAll('path').find((p) => p.attributes('stroke') === 'currentColor')

  it('有曲线后用 0 垫满窗口左侧：0 线与真实曲线相接（峰值贴顶 / 零值贴底）', () => {
    const w = mount(TrafficSparkline, { props: { values: [50, 100] } })
    const paths = w.findAll('path')
    expect(paths).toHaveLength(2)

    const d = lineOf(w)!.attributes('d')!
    expect(d.startsWith('M0.00,22.00')).toBe(true)
    // 最后一个 0 点（x=96.61）紧接首个真实样本（x=98.31，y=12），再收于峰值
    expect(d).toContain('L96.61,22.00 L98.31,12.00')
    expect(d.endsWith('L100.00,2.00')).toBe(true)

    const area = paths.find((p) => p.attributes('fill') === 'currentColor')!
    expect(area.attributes('d')!.startsWith(d)).toBe(true)
    expect(area.attributes('d')!.endsWith('Z')).toBe(true)
    w.unmount()
  })

  it('窗口容量 60：超出丢弃最老样本，满窗口起点落在最左', () => {
    const values = Array.from({ length: 70 }, (_, i) => i + 1)
    const w = mount(TrafficSparkline, { props: { values } })
    const d = lineOf(w)!.attributes('d')!
    expect(d.match(/L/g)).toHaveLength(59) // 60 点 → 59 段
    expect(d.startsWith('M0.00,')).toBe(true)
    w.unmount()
  })

  it('冷启动无曲线：铺 0 横线占位（不渲染面积），单点仍保持占位', async () => {
    const w = mount(TrafficSparkline, { props: { values: [] } })
    expect(w.findAll('path')).toHaveLength(1)
    expect(lineOf(w)!.attributes('d')).toBe('M0,22 L100,22')

    await w.setProps({ values: [123] })
    expect(w.findAll('path')).toHaveLength(1)
    expect(lineOf(w)!.attributes('d')).toBe('M0,22 L100,22')
    w.unmount()
  })

  it('全零速率：平线落在底边', () => {
    const w = mount(TrafficSparkline, { props: { values: [0, 0, 0] } })
    expect(lineOf(w)!.attributes('d')!.endsWith('L100.00,22.00')).toBe(true)
    w.unmount()
  })
})
