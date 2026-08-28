import { describe, it, expect } from 'vitest'
import { hitTestShape, findShapeAt } from './shapeHitTest'
import type { Shape } from './useTypes'

function rect(x1: number, y1: number, x2: number, y2: number): Shape {
  return {
    type: 'rect',
    x1,
    y1,
    x2,
    y2,
    color: '#000',
    lineWidth: 2,
  }
}

function line(x1: number, y1: number, x2: number, y2: number): Shape {
  return {
    type: 'line',
    x1,
    y1,
    x2,
    y2,
    color: '#000',
    lineWidth: 2,
  }
}

describe('hitTestShape', () => {
  it('命中矩形边框附近', () => {
    const s = rect(10, 10, 100, 80)
    expect(hitTestShape(s, 10, 40)).toBe(true) // 左边
    expect(hitTestShape(s, 55, 10)).toBe(true) // 顶边
    expect(hitTestShape(s, 50, 45)).toBe(false) // 内部不命中（仅边）
  })

  it('命中直线', () => {
    const s = line(0, 0, 100, 0)
    expect(hitTestShape(s, 50, 0)).toBe(true)
    expect(hitTestShape(s, 50, 20)).toBe(false)
  })

  it('命中 blur 区域（填充）', () => {
    const s: Shape = {
      type: 'blur',
      x1: 0,
      y1: 0,
      x2: 50,
      y2: 50,
      color: '#000',
      lineWidth: 1,
    }
    expect(hitTestShape(s, 25, 25)).toBe(true)
    expect(hitTestShape(s, 60, 60)).toBe(false)
  })

  it('标签底色文字：命中区随底色内边距外扩', () => {
    const s: Shape = {
      type: 'text',
      x1: 0,
      y1: 0,
      x2: 0,
      y2: 0,
      color: '#ff3b30',
      lineWidth: 1,
      fontSize: 20,
      text: '标签',
      textLines: ['标签'],
      textWidth: 40,
      textBg: true,
    }
    // fontSize 20 → padX = 7，底色左边缘在 x1-7；HIT=8 外的 -10 仅底色模式命中
    expect(hitTestShape(s, -10, 13)).toBe(true)
    expect(hitTestShape({ ...s, textBg: undefined }, -10, 13)).toBe(false)
  })
})

describe('findShapeAt', () => {
  it('自上而下返回最上层命中', () => {
    const shapes = [rect(0, 0, 100, 100), line(0, 50, 100, 50)]
    // 点在矩形边与直线交汇附近：后绘的 line 应优先
    expect(findShapeAt(shapes, 50, 50)).toBe(1)
    expect(findShapeAt(shapes, 0, 0)).toBe(0) // 仅矩形角
    expect(findShapeAt(shapes, 200, 200)).toBe(-1)
  })
})
