import { describe, it, expect } from 'vitest'
import { wrapIndex, isComposing, isFormControl } from './dom'

describe('wrapIndex', () => {
  it('向下循环', () => {
    expect(wrapIndex(0, 3, 'down')).toBe(1)
    expect(wrapIndex(1, 3, 'down')).toBe(2)
    expect(wrapIndex(2, 3, 'down')).toBe(0)
  })

  it('向上循环', () => {
    expect(wrapIndex(2, 3, 'up')).toBe(1)
    expect(wrapIndex(1, 3, 'up')).toBe(0)
    expect(wrapIndex(0, 3, 'up')).toBe(2)
  })

  it('空列表返回 0', () => {
    expect(wrapIndex(0, 0, 'down')).toBe(0)
    expect(wrapIndex(0, 0, 'up')).toBe(0)
  })

  it('单元素列表', () => {
    expect(wrapIndex(0, 1, 'down')).toBe(0)
    expect(wrapIndex(0, 1, 'up')).toBe(0)
  })
})

describe('isComposing', () => {
  it('正常事件返回 false', () => {
    expect(isComposing(new KeyboardEvent('keydown'))).toBe(false)
  })
})

describe('isFormControl', () => {
  it('null/undefined 返回 false', () => {
    expect(isFormControl(null)).toBe(false)
    expect(isFormControl(undefined)).toBe(false)
  })

  it('INPUT 元素返回 true', () => {
    const input = document.createElement('input')
    expect(isFormControl(input)).toBe(true)
  })

  it('TEXTAREA 元素返回 true', () => {
    const textarea = document.createElement('textarea')
    expect(isFormControl(textarea)).toBe(true)
  })

  it('普通 DIV 返回 false', () => {
    const div = document.createElement('div')
    expect(isFormControl(div)).toBe(false)
  })

  it('contenteditable 元素返回 true', () => {
    const div = document.createElement('div')
    div.setAttribute('contenteditable', 'true')
    expect(isFormControl(div)).toBe(true)
  })

  it('data-settings-control 检查', () => {
    const div = document.createElement('div')
    div.setAttribute('data-settings-control', '')
    expect(isFormControl(div, { settingsControl: true })).toBe(true)
    expect(isFormControl(div)).toBe(false)
  })
})
