import { describe, it, expect } from 'vitest'
import { contrastInk, textBgHPad, textBgRadius } from './useTypes'

describe('contrastInk', () => {
  it('暗底白字 / 亮底黑字', () => {
    expect(contrastInk('#ff3b30')).toBe('#ffffff')
    expect(contrastInk('#007aff')).toBe('#ffffff')
    expect(contrastInk('#000000')).toBe('#ffffff')
    expect(contrastInk('#ffcc00')).toBe('#000000')
    expect(contrastInk('#34c759')).toBe('#000000')
    expect(contrastInk('#ffffff')).toBe('#000000')
  })
})

describe('标签底色几何', () => {
  it('内边距随字号缩放', () => {
    expect(textBgHPad(20)).toBe(7)
    expect(textBgHPad(24)).toBe(8)
  })

  it('圆角不超过半行高且保底 4', () => {
    expect(textBgRadius(12, 16)).toBe(4)
    expect(textBgRadius(64, 83)).toBe(19)
  })
})
