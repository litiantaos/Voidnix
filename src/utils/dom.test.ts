import { describe, it, expect } from 'vitest'
import { wrapIndex, isComposing, isFormControl, resumeInfiniteAnimations } from './dom'

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

describe('resumeInfiniteAnimations', () => {
  // happy-dom 无动画引擎，stub document.getAnimations（duck-typed 假动画）
  function stubAnimations(anims: unknown[]) {
    Object.defineProperty(document, 'getAnimations', {
      value: () => anims,
      configurable: true,
    })
  }
  const drop = (obj: object, key: string) => delete (obj as Record<string, unknown>)[key]

  it('重启无限循环动画（none → forced reflow → 还原），有限动画不触碰', () => {
    const spinner = document.createElement('i')
    const once = document.createElement('i')
    document.body.append(spinner, once)
    stubAnimations([
      { effect: { getTiming: () => ({ iterations: Infinity }), target: spinner } },
      { effect: { getTiming: () => ({ iterations: 1 }), target: once } },
    ])

    // 拦截 forced reflow：捕获 reflow 时刻两目标的中间态
    const atReflow: string[] = []
    Object.defineProperty(document.body, 'offsetHeight', {
      configurable: true,
      get: () => {
        atReflow.push(`${spinner.style.animationName}|${once.style.animationName}`)
        return 0
      },
    })

    try {
      expect(resumeInfiniteAnimations()).toBe(1)
    } finally {
      drop(document.body, 'offsetHeight')
      drop(document, 'getAnimations')
    }

    expect(atReflow).toEqual(['none|'])
    // 还原后 inline 覆写清空（交回样式表值），动画注销重建
    expect(spinner.style.animationName).toBe('')
    expect(once.style.animationName).toBe('')
  })

  it('无无限循环动画：零重启', () => {
    stubAnimations([{ effect: { getTiming: () => ({ iterations: 1 }), target: document.body } }])
    expect(resumeInfiniteAnimations()).toBe(0)
    drop(document, 'getAnimations')
  })
})
